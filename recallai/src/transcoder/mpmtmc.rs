use crate::data::{Dim, InHandle, OutHandle};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::thread::{scope, ScopedJoinHandle};

use crate::meta::MetaHeader;
use crate::rules::PartialRules;
use crate::spec::VidFileHeader;
use crate::TranscodeError;

// This encoder showed promise, however with UHD frames, we are thrashing L3 cache
// So it winds up just firing off millions of context switches (according to /usr/bin/time -v)
// vs the other methods.  At 5 parallel workers, we have 1M ctx switches v.s. only 37k for SeqMt
//
// The original theory of operation was that a CPU could have a dedicated nvme transfer-lane from
// IO channel to the L2 cache.. It would process said data loacally, then submit the modified results
// directly to the nvme lane.. Each CPU could work in parallel. HOWEVER, at 26MB per frame, AND
// the need to randomly manipulate arbitrary source and target locations; there just isn't enough
// locality or streamability to make this worth while. Further, this would really need something like
// "io_uring" and glommio to truely isolate the kernel threads to the CPU cores.
//
// As it stands, this is the slowest engine for LARGE frame buffers (I haven't tested on 128KB buffers)
//
// The architecture is as follows:
// N workers, each with shared-nothing (in process) resources.
// Each using random-access readers and writers to read from a given frame and write to one.
// This would NOT be efficient on "NTFS" as that is a dense file system and any writes beyond the end
// of the file would stall; waiting for zeros to be emitted.
// In LINUX, however, both EXT3 and EXT4 (and I believe XFS) support highly efficient out-of-order
// sparse-files... In my experience, to maximize throughput (minimizing delays) use O_DIRECT and
// asyncio (io_uring might have caught up with this).  It seems to shave an extra 15% off the kernel time.
// This was NOT implemented here
//
pub struct MpMtMcTranscoder {
    // pub meta_header: MetaHeader,
    pub rules: Option<PartialRules>,
    pub num_cpu_workers: u16,
}

impl MpMtMcTranscoder {
    pub(crate) fn transcode_handles(
        &mut self,
        channel_in: InHandle,
        channel_out: OutHandle,
    ) -> Result<(), TranscodeError> {
        match (channel_in, channel_out) {
            (InHandle::File(file_in), OutHandle::File(file_out)) => {
                self.transcode_file_names(file_in, file_out)
            }
            _ => Err(TranscodeError::Unsupported),
        }
    }

    pub(crate) fn transcode_file_names(
        &mut self,
        file_name_in: String,
        file_name_out: String,
    ) -> Result<(), TranscodeError> {
        let (meta_header, rules) = MetaHeader::from_file_and_partial_rules(
            &file_name_in,
            self.rules.take().ok_or(TranscodeError::Unsupported)?,
        )?;
        let src_capacity = meta_header.frame_size;
        let tgt_dim: Dim = rules.get_tgt_dim();
        let tgt_capacity = tgt_dim.calc_size();
        let num_frames = meta_header.num_frames;

        let tgt_frame_header = VidFileHeader {
            dim: tgt_dim,
            num_frames: num_frames,
        };
        let mut main_out = File::create(&file_name_out).map_err(TranscodeError::IoErr)?;
        tgt_frame_header.write_header(&mut main_out)?;
        let num_cpu_workers = self.num_cpu_workers;

        let out_frame = AtomicU32::new(0);
        let rules_ref = &rules;

        let answers = scope(|s| {
            let mut answer_vec = Vec::with_capacity(num_cpu_workers as usize);
            for _ in 0..num_cpu_workers {
                let fut: ScopedJoinHandle<Result<(), TranscodeError>> = s.spawn(|| {
                    let mut file_in = File::open(&file_name_in).map_err(TranscodeError::IoErr)?;

                    let mut file_out = File::options()
                        .write(true)
                        .open(&file_name_out)
                        .map_err(TranscodeError::IoErr)?;

                    let mut src_buf = vec![0u8; src_capacity].into_boxed_slice();
                    let mut tgt_buf = vec![0u8; tgt_capacity].into_boxed_slice();

                    loop {
                        let my_frame = out_frame.fetch_add(1, SeqCst);
                        if my_frame >= num_frames {
                            break;
                        }
                        seek_file(&mut file_in, src_capacity, my_frame)?;
                        file_in
                            .read_exact(&mut src_buf)
                            .map_err(TranscodeError::IoErr)?;
                        seek_file(&mut file_out, tgt_capacity, my_frame)?;
                        rules_ref.process_frame(my_frame, &src_buf, &mut tgt_buf)?;
                        file_out
                            .write_all(&tgt_buf)
                            .map_err(TranscodeError::IoErr)?;
                    }
                    Ok(())
                });
                answer_vec.push(fut);
            }
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
