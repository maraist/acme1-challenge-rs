use crate::data::{InHandle, OutHandle};
use crossbeam_channel::{Receiver, Sender};
use std::fs::File;
use std::io::{stdin, stdout, Read, Seek, SeekFrom, Write};
use std::thread::{scope, ScopedJoinHandle};

use crate::meta::MetaHeader;
use crate::rules::PartialRules;
use crate::spec::VidFileHeader;
use crate::TranscodeError;

// This is our preferred encoder
//
// This has a very primitive dedicated reader and dedicated writer thread pair. It contains
// N compute workers.. It uses a MPSC and SPMC suite of exchange queues (thanks to crossbeam).
// While I would normally rather stick with std-rust, the lack of XXMC channels was taking too
// much time to emulate; the reason to stick to std is that every year or so, they dramatically increase
// its performance.
//
// Similar to parmt, this has a random-access reader and writer; and thus relies on efficient sparse-file-systems
// such as EXT3 and EXT4.  For the LARGE UHD frames, I do not believe it is necessary to leverage fallocate.
// Normally such random access writes would trigger file fragmentation; but beyond 2MB frames; this should
// be fine.  "filefrag" only reports 433 extents for the 1000 frame file on EXT4. Not great, but not bad.
//
// We use scoped threads instead of a thread-pool; because I don't like work-stealing non-determinism.
// Unfortunately, this has the least robust error management of the three schedulers. There are more
// complex ways an IO error or queue error can manifest, and I'm pretty sure I have NOT handled
// all the edge cases; I've resorted to "panic".. This is a TODO.. Given more sleep; I'd address this.
//
// It seems (at least on my machine) 3 workers can saturate my L3.
// 1 thread : 34sec user-time, 8s sys-time, 42s wall
// 2 threads: 38sec user-time, 8.5s sys-time, 25s wall
// 3 threads: 41sec user-time, 9.5s sys-time, 18s wall
// 4 threads: 45sec user-time, 10.0s sys-time, 16.5s wall
// 5 threads: 51sec user-time, 11.0s sys-time, 15.5s wall
//
// My estimate, RAM Cache -> L2 Cache -> L3 Cache (spill) -> (ctx switch) -> L2 Cache pull ->
// process -> L3 Cache (spill) -> RAM Cache (on the write)
//
// My preference would be to minimize the number of RAM / L3 passes; but I'm somewhat daunted..
//
// On an x16 with 32GB of RAM, this should be sufficient
pub struct SpMtScTranscoder {
    // pub meta_header: MetaHeader,
    pub rules: Option<PartialRules>,
    pub num_cpu_workers: u16,
}

impl SpMtScTranscoder {
    pub(crate) fn transcode_handles(
        &mut self,
        channel_in: InHandle,
        channel_out: OutHandle,
    ) -> Result<(), TranscodeError> {
        match (channel_in, channel_out) {
            (InHandle::File(file_in), OutHandle::File(file_out)) => {
                self.transcode_file_names(&file_in, &file_out)
            }
            (InHandle::Stdin, OutHandle::File(file_out)) => {
                self.transcode_file_names("-", &file_out)
            }
            (InHandle::File(file_in), OutHandle::Stdout) => {
                self.transcode_file_names(&file_in, "-")
            }
            (InHandle::Stdin, OutHandle::Stdout) => self.transcode_file_names("-", "-"),
            _ => Err(TranscodeError::Unsupported),
        }
    }

    pub(crate) fn transcode_file_names(
        &mut self,
        file_name_in: &str,
        file_name_out: &str,
    ) -> Result<(), TranscodeError> {
        let (meta_header, rules) = MetaHeader::from_file_and_partial_rules(
            file_name_in,
            self.rules.take().ok_or(TranscodeError::Unsupported)?,
        )?;
        // let meta_header = self.meta_header;
        let src_capacity = meta_header.frame_size;
        let tgt_dim = rules.get_tgt_dim();
        let tgt_capacity = tgt_dim.calc_size();
        let num_frames = meta_header.num_frames;

        let num_cpu_workers = self.num_cpu_workers;

        let rules_ref = &rules;

        let answers = scope(|s| {
            let mut answer_vec = Vec::with_capacity(num_cpu_workers as usize);

            let free_src_buffers: (Sender<Box<[u8]>>, Receiver<Box<[u8]>>) =
                crossbeam_channel::unbounded();
            let free_tgt_buffers: (Sender<Box<[u8]>>, Receiver<Box<[u8]>>) =
                crossbeam_channel::unbounded();
            let full_src_buffers: (Sender<(u32, Box<[u8]>)>, Receiver<(u32, Box<[u8]>)>) =
                crossbeam_channel::unbounded();
            let full_tgt_buffers: (Sender<(u32, Box<[u8]>)>, Receiver<(u32, Box<[u8]>)>) =
                crossbeam_channel::unbounded();

            for _ in 0..num_cpu_workers {
                let full_tgt_buffer_snd = full_tgt_buffers.0.clone();
                let free_tgt_buffer_rcv = free_tgt_buffers.1.clone();
                let full_src_buffer_rcv = full_src_buffers.1.clone();
                let free_src_buffer_snd = free_src_buffers.0.clone();
                let fut: ScopedJoinHandle<Result<(), TranscodeError>> = s.spawn(move || {
                    // have worker threads init, allocate and zero to startup faster
                    let src_buf_ = vec![0u8; src_capacity].into_boxed_slice();
                    let tgt_buf_ = vec![0u8; tgt_capacity].into_boxed_slice();
                    let _ = free_src_buffer_snd.send(src_buf_);

                    let mut initial_tgt_buf = Some(tgt_buf_);

                    loop {
                        let mut tgt_buf = if let Some(t) = initial_tgt_buf.take() {
                            t
                        } else if let Ok(t) = free_tgt_buffer_rcv.recv() {
                            t
                        } else {
                            return Err(TranscodeError::ThreadErr);
                        };

                        let (fr, src_buf) = if let Ok((fr, src_buf)) = full_src_buffer_rcv.recv() {
                            (fr, src_buf)
                        } else {
                            // graceful shutdown
                            return Ok(());
                        };

                        let _ = rules_ref.process_frame(fr, &src_buf, tgt_buf.as_mut())?;
                        let _ = full_tgt_buffer_snd
                            .send((fr, tgt_buf))
                            .or_else(|_| Err(TranscodeError::CrossBeamErr))?;
                        if let Err(_) = free_src_buffer_snd.send(src_buf) {
                            // slightly ungraceful shutdown
                            // ignore any errors; we're shutting down
                        }
                    }
                });
                answer_vec.push(fut);
            }

            let (free_tgt_buffer_snd, _) = free_tgt_buffers;
            let (_, full_tgt_buffer_rcv) = full_tgt_buffers;
            // writer thread
            let answer = s.spawn(move || {
                let tgt_frame_header = VidFileHeader {
                    dim: tgt_dim,
                    num_frames: num_frames,
                };

                // todo, dedup
                if file_name_out == "-" {
                    let mut file_out = stdout().lock();
                    tgt_frame_header.write_header(&mut file_out)?;
                    let mut stash = vec![];
                    let mut next_fr = 0;

                    while let Ok((fr, tgt_buf)) = full_tgt_buffer_rcv.recv() {
                        if fr != next_fr {
                            stash.push((fr, tgt_buf));
                            stash.sort_by(|a, b| a.0.cmp(&b.0));
                            continue;
                        }

                        let mut tgt_buf = tgt_buf;
                        loop {
                            file_out
                                .write_all(&tgt_buf)
                                .map_err(TranscodeError::IoErr)?;
                            if let Err(_) = free_tgt_buffer_snd.send(tgt_buf) {
                                // ignore ungraceful shutdown
                            }
                            next_fr += 1;
                            if !stash.is_empty() && stash[0].0 == next_fr {
                                let (_, tgt_buf_) = stash.remove(0);
                                tgt_buf = tgt_buf_;
                                continue;
                            } else {
                                break;
                            }
                        }
                    }
                } else {
                    let mut file_out =
                        File::create(file_name_out).map_err(TranscodeError::IoErr)?;
                    tgt_frame_header.write_header(&mut file_out)?;

                    while let Ok((fr, tgt_buf)) = full_tgt_buffer_rcv.recv() {
                        seek_file(&mut file_out, tgt_capacity, fr)?;
                        file_out
                            .write_all(&tgt_buf)
                            .map_err(TranscodeError::IoErr)?;
                        if let Err(_) = free_tgt_buffer_snd.send(tgt_buf) {
                            // ignore ungraceful shutdown
                        }
                    }
                };

                Ok(())
            });
            answer_vec.push(answer);

            let (full_src_buffer_snd, _) = full_src_buffers;
            // Reader thread
            let (_, free_src_buffer_rcv) = free_src_buffers;
            let answer = s.spawn(move || {
                // main reader thread
                // TODO dedup
                if file_name_in == "-" {
                    let mut file_in = stdin().lock();
                    for fr in 0..num_frames {
                        let Ok(mut src_buf) = free_src_buffer_rcv.recv() else {
                            return Err(TranscodeError::ThreadErr);
                        };
                        file_in
                            .read_exact(&mut src_buf)
                            .map_err(TranscodeError::IoErr)?;
                        let _ = full_src_buffer_snd
                            .send((fr, src_buf))
                            .or_else(|_| Err(TranscodeError::CrossBeamErr))?;
                    }
                } else {
                    let mut file_in = File::open(file_name_in).map_err(TranscodeError::IoErr)?;
                    for fr in 0..num_frames {
                        let Ok(mut src_buf) = free_src_buffer_rcv.recv() else {
                            return Err(TranscodeError::ThreadErr);
                        };
                        seek_file(&mut file_in, src_capacity, fr)?;
                        file_in
                            .read_exact(&mut src_buf)
                            .map_err(TranscodeError::IoErr)?;
                        let _ = full_src_buffer_snd
                            .send((fr, src_buf))
                            .or_else(|_| Err(TranscodeError::CrossBeamErr))?;
                    }
                }
                Ok(())
            });

            let r = answer.join();
            if let Err(e) = r {
                println!("reader thread has error {e:?}");
            }

            // we need to drop once the reader is done
            drop(full_tgt_buffers.0);

            let answers: Vec<_> = answer_vec.into_iter().map(|j| j.join()).collect();
            answers
        });

        collect_errors(answers)
    }
}

fn collect_errors<T>(
    responses: Vec<Result<Result<(), TranscodeError>, T>>,
) -> Result<(), TranscodeError> {
    let mut last_err = None;
    // report errors
    for answer in responses.into_iter() {
        match answer {
            Err(_) => {
                println!("thread processing error");
                last_err = Some(TranscodeError::ThreadErr);
            }
            Ok(inner) => {
                match inner {
                    Ok(_) => {
                        // good
                    }
                    Err(e) => {
                        println!("thread returned error {:?}", e);
                        last_err = Some(e);
                    }
                }
            }
        }
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn seek_pos(frame_size: usize, frame: u32) -> u64 {
    (VidFileHeader::get_header_size() as u64) + (frame_size as u64) * (frame as u64)
}

fn seek_file(rdr: &mut File, frame_size: usize, frame: u32) -> Result<(), TranscodeError> {
    rdr.seek(SeekFrom::Start(seek_pos(frame_size, frame)))
        .map_err(TranscodeError::IoErr)?;
    Ok(())
}
