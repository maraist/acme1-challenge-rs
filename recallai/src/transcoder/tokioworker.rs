use std::io::Cursor;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::Instant;

use async_stream::try_stream;
use bytes::Bytes;
use futures::Stream;
use image::EncodableLayout;
use reqwest::{Body, Client};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Handle;
use ulid::Ulid;

use crate::data::{InHandle, OutHandle};
use crate::meta::MetaHeader;
use crate::rules::{PartialRules, RuleOpts, Rules};
use crate::spec::VidFileHeader;
use crate::transcoder::bufstack::BufStack;
use crate::TranscodeError;

// This is a prototype for a tokio worker pool.
// After working this through, there are many things I would do differently.
// Specifically the Bytes object is cloneable and thus reusable.. As written, I relinquish
// control of the Bytes buffer to the reqwest Body writer, and thus must create a new one
// for each frame. The goal would be to reuse, just as in all the other workflows - which tokio
// and hyper allow - but it wasn't obvious how to get reqwest to allow it at the time.
//
// This general flow is similar to spmtsc except that we use Bytes objects instead of Vecs, and
// we need to extract the Vec from the Bytes object in the producer side (it's basically a noop so long as
// the concurrent-usage-count is exactly-1 ; which should be the case for this workflow).
//
// The main difference between the other workflows is that this supports reqwest for HTTPS async IO.
// The TLS implementation makes everything more complicated than for basic file or stream IO.
//
// Unlike other polymorphic transcoder varients, this accepts a JOB ID.
//
// Note, unlike all the other code modules, this was created in a rush,
// so I did NOT handle the errors. I just unwrap
// This is a MAJOR TODO

// To keep things more readible, I use Read / Write Marker structs to wrap the Bytes.
struct WBytes(Bytes);

impl WBytes {
    fn new(size: usize) -> Self {
        Self(Bytes::from(vec![0u8; size]))
    }
    fn from(buf: Vec<u8>) -> Self {
        Self(Bytes::from(buf))
    }
}

impl From<Vec<u8>> for WBytes {
    fn from(value: Vec<u8>) -> Self {
        Self(Bytes::from(value))
    }
}

struct RBytes(Bytes);

impl From<Vec<u8>> for RBytes {
    fn from(value: Vec<u8>) -> Self {
        Self(Bytes::from(value))
    }
}

impl RBytes {
    fn new(size: usize) -> Self {
        Self(Bytes::from(vec![0u8; size]))
    }
    fn empty() -> Self {
        Self(Bytes::new())
    }
}

// TODO, this is probably consolidatable with WBytesStack
struct RBytesStack {
    stack: Vec<RBytes>,
    sz: usize,
}

impl crate::transcoder::tokioworker::RBytesStack {
    fn new(sz: usize) -> Self {
        crate::transcoder::tokioworker::RBytesStack { stack: vec![], sz }
    }
    fn pop(&mut self) -> RBytes {
        if let Some(b) = self.stack.pop() {
            b
        } else {
            RBytes::new(self.sz)
        }
    }
    fn push(&mut self, buf: RBytes) {
        self.stack.push(buf);
    }
}

struct WBytesStack {
    stack: Vec<WBytes>,
    sz: usize,
}

impl WBytesStack {
    fn new(sz: usize) -> Self {
        WBytesStack { stack: vec![], sz }
    }
    fn pop(&mut self) -> WBytes {
        if let Some(b) = self.stack.pop() {
            b
        } else {
            WBytes::new(self.sz)
        }
    }
    fn push(&mut self, buf: WBytes) {
        self.stack.push(buf);
    }
}

// This is the workflow pipeline descriptor
pub struct TokioTranscoder {
    pub num_workers: u16,
    // pub meta_header: Option<MetaHeader>,
    pub rules: Option<PartialRules>,
    pub debug: bool,
    pub collect_stats: bool,
    pub jobid: Ulid,
}

//
// These three messages represent a back-pressure circular buffer ring.
//

// (frame,src-buf, tgt-buf)
struct ConsumerMessage(u32, Vec<u8>, Vec<u8>);

struct TranscoderMessage(
    // frame
    u32,
    // src buffer
    Vec<u8>,
    // ttgt transcoded buffer
    Vec<u8>,
    // we avoid Arc by sending a 'thin' clone of the channel with each message
    // None means the stream is done
    tokio::sync::mpsc::Sender<Option<ConsumerMessage>>,
);

impl TokioTranscoder {
    pub(crate) async fn transcode_channels<R, W>(
        &mut self,
        channel_in: R,
        channel_out: W,
    ) -> Result<(u32, u32), TranscodeError>
    where
        R: tokio::io::AsyncReadExt + Unpin,
        W: tokio::io::AsyncWriteExt + Unpin + Send + 'static,
    {
        let rules = self.rules.take().ok_or(TranscodeError::Unsupported)?;
        let (meta_header, mut channel_in) = TokioTranscoder::read_header(channel_in).await?;
        let rules = rules.build(meta_header)?;

        let mut channel_out =
            TokioTranscoder::write_header(&meta_header, &rules, channel_out).await?;

        if self.num_workers > 1 {
            return self
                .parallel(meta_header, rules, channel_in, channel_out)
                .await;
        }
        let jobid = self.jobid;
        let debug = self.debug;

        let mut in_buf = vec![0u8; meta_header.frame_size];
        let out_buf_sz = rules.get_tgt_dim().calc_size();
        let mut out_buf = vec![0u8; out_buf_sz];

        let mut min_time = u128::MAX;
        let mut max_time = 0;
        let mut sum_time = 0;
        let num_frames = meta_header.num_frames;

        // Given that this is an async reader, we assume the input header is already consumed
        for fr in 0..num_frames {
            let num_read = channel_in
                .read_exact(&mut in_buf)
                .await
                .or_else(|_| Err(TranscodeError::InputTruncated))?;
            if num_read != meta_header.frame_size {
                return Err(TranscodeError::InputTruncated);
            }
            let st = if debug || self.collect_stats {
                Some(Instant::now())
            } else {
                None
            };

            rules.process_frame(fr, &in_buf, &mut out_buf)?;
            st.map(|v| {
                let took = Instant::now().duration_since(v).as_millis();
                min_time = min_time.min(took);
                max_time = max_time.max(took);
                sum_time += took;
                if debug {
                    println!("job:{jobid} fr:{fr} main: took:{took}");
                }
            });
            channel_out
                .write_all(&out_buf)
                .await
                .map_err(|_| TranscodeError::OutputTruncated)
                .or_else(|_| Err(TranscodeError::Unsupported))?;
        }
        if self.collect_stats {
            let avg = sum_time as f32 / num_frames as f32;
            println!("job: {jobid}  min:{min_time} max:{max_time} avg:{avg}");
        }
        Ok((num_frames, num_frames))
    }
    async fn parallel<R, W>(
        &mut self,
        meta_header: MetaHeader,
        rules: Rules,
        channel_in: R,
        mut channel_out: W,
    ) -> Result<(u32, u32), TranscodeError>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let jobid = self.jobid;
        println!("job:{jobid} started parallel");
        let st = Instant::now();
        let num_workers = self.num_workers as usize;
        let num_frames = meta_header.num_frames;
        let debug = self.debug;
        let collect_stats = self.collect_stats;
        let mut in_buf_stack = BufStack::new(meta_header.frame_size);
        let mut out_buf_stack = BufStack::new(rules.get_tgt_dim().calc_size());

        let (wtx, wrx) = tokio::sync::mpsc::channel::<Option<ConsumerMessage>>(num_workers);
        let (ctx, crx) = crossbeam_channel::bounded::<TranscoderMessage>(num_workers);
        let (rtx, rrx) = tokio::sync::mpsc::channel::<ConsumerMessage>(num_workers);

        // Spawn CPU processors in dedicated Rayon thread-pool
        let rule_arc = Arc::new(rules);
        let mut cpu_futs = vec![];
        for worker_id in 0..num_workers {
            let crx_ = crx.clone();
            let rules_ = rule_arc.clone();
            let t = tokio::task::spawn_blocking(move || {
                if debug {
                    println!("job: {jobid} worker {worker_id} CPU started");
                }
                let mut min_time = u128::MAX;
                let mut max_time = 0;
                let mut sum_time = 0;
                let mut num_processed = 0;
                let mut last_sent = None;

                while let Ok(TranscoderMessage(fr, in_fr, mut out_fr, nxt)) = crx_.recv() {
                    let st = if debug || collect_stats {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    let Ok(_) = rules_.process_frame(fr, &in_fr, &mut out_fr) else {
                        println!("bad");
                        return;
                    };
                    num_processed += 1;
                    st.map(|v| {
                        let took = Instant::now().duration_since(v).as_millis();
                        min_time = min_time.min(took);
                        max_time = max_time.max(took);
                        sum_time += took;
                        if debug {
                            println!(
                                "job:{jobid} worker {worker_id} fr:{fr} worker: took:{}",
                                Instant::now().duration_since(v).as_millis()
                            );
                        }
                    });
                    let Ok(_) = nxt.blocking_send(Some(ConsumerMessage(fr, in_fr, out_fr))) else {
                        println!("bad");
                        return;
                    };
                    last_sent = Some(nxt);
                }
                let avg = sum_time as f32 / num_processed as f32;
                if debug || collect_stats {
                    println!("job: {jobid} worker {worker_id} CPU ended; min:{min_time} max:{max_time} avg:{avg}");
                }
                if let Some(last_sent) = last_sent {
                    let Ok(_) = last_sent.blocking_send(None) else {
                        println!("bad");
                        return;
                    };
                }
            });
            cpu_futs.push(t);
        }

        // Spawn BG writer Consumer (in tokio thread)
        let writer_fut = tokio::spawn(async move {
            let mut num_written = 0;
            let mut wrx = wrx;
            let mut reorder: Vec<ConsumerMessage> = vec![]; // used to handle out of order inbounds

            let mut num_exited = 0;
            while let Some(msg) = wrx.recv().await {
                if let Some(ConsumerMessage(mut fr, mut in_buf, mut out_buf)) = msg {
                    loop {
                        if num_written != fr {
                            if debug {
                                println!("job: {jobid} fr: {fr} writer: out of order");
                            }
                            reorder.push(ConsumerMessage(fr, in_buf, out_buf));
                            reorder.sort_by(|a, b| a.0.cmp(&b.0));
                            break;
                        } else {
                            // reorder
                            // TODO handle errors better
                            channel_out
                                .write_all(&out_buf)
                                .await
                                .map_err(|_| TranscodeError::OutputTruncated)
                                .unwrap();
                            num_written += 1;
                            if let Err(_) = rtx.send(ConsumerMessage(fr, in_buf, out_buf)).await {
                                // hopefully near end of stream
                                println!("job: {jobid} fr: {fr} writer: couldnt send to reader; probably exiting");
                            }
                            if !reorder.is_empty() && reorder[0].0 == num_written {
                                let ConsumerMessage(fr_, in_buf_, out_buf_) = reorder.remove(0);
                                fr = fr_;
                                in_buf = in_buf_;
                                out_buf = out_buf_;
                                if debug {
                                    println!("job: {jobid} fr: {fr} writer: re-ordering");
                                }
                            } else {
                                break;
                            }
                            if num_written == num_frames {
                                if debug {
                                    println!(
                                        "job: {jobid} writer: done; num_written: {num_written}"
                                    );
                                }
                                return num_written;
                            }
                        }
                    }
                } else {
                    num_exited += 1;
                    if num_exited == num_workers {
                        println!("job: {jobid} writer: early exit detected; all workers done; exiting; num_written:{num_written}");
                        return num_written;
                    } else {
                        println!("job: {jobid} writer: early exit detected");
                    }
                }
            }
            if debug {
                println!(
                    "job: {jobid} writer: premature exit! only wrote num_written: {num_written}"
                );
            }
            return num_written;
        });

        // Main thread reader/Producer
        {
            let mut num_outstanding = 0;
            let mut rrx = rrx;
            let mut channel_in = channel_in;
            // Given that this is an async reader, we assume the input header is already consumed
            for fr in 0..num_frames {
                let (mut in_buf, out_buf) = if num_outstanding >= num_workers {
                    let ConsumerMessage(fr, in_buf, out_buf) = rrx.recv().await.unwrap();
                    if self.debug {
                        println!("job:{jobid} fr:{fr} main: fully finished frame");
                    }
                    (in_buf, out_buf)
                } else {
                    if debug {
                        println!("main(rdr) creating {fr}");
                    }
                    num_outstanding += 1;
                    let in_buf = in_buf_stack.pop();
                    let out_buf = out_buf_stack.pop();
                    (in_buf, out_buf)
                };

                let num_read = channel_in.read_exact(&mut in_buf).await.unwrap();
                if num_read != meta_header.frame_size {
                    rrx.close();
                    return Err(TranscodeError::InputTruncated);
                }
                let _ = ctx
                    .send(TranscoderMessage(fr, in_buf, out_buf, wtx.clone()))
                    .unwrap();
            }
            drop(ctx); // shuts down writer to CPU
        }
        let num_written = writer_fut.await.unwrap();
        if self.debug || self.collect_stats {
            println!(
                "job:{jobid} finished took: {}ms",
                Instant::now().duration_since(st).as_millis()
            );
        }

        for cpu_fut in cpu_futs.into_iter() {
            let _ = cpu_fut.await.unwrap();
        }

        if num_written != num_frames {
            println!("write mismatch ; wrote {num_written} expected {num_frames}");
            Err(TranscodeError::OutputTruncated)
        } else {
            Ok((num_written, num_frames))
        }
    }

    async fn write_header<W>(
        md: &MetaHeader,
        rules: &Rules,
        mut channel_out: W,
    ) -> Result<W, TranscodeError>
    where
        W: tokio::io::AsyncWriteExt + Unpin,
    {
        let tgt_frame_header = VidFileHeader {
            dim: rules.get_tgt_dim(),
            num_frames: md.num_frames,
        };
        let hdr_buf = tgt_frame_header.create_header();
        channel_out
            .write_all(&hdr_buf)
            .await
            .map_err(|_| TranscodeError::OutputTruncated)?;
        Ok(channel_out)
    }
    async fn read_header<R>(mut channel_in: R) -> Result<(MetaHeader, R), TranscodeError>
    where
        R: tokio::io::AsyncReadExt + Unpin,
    {
        let mut hdr_buf = vec![0u8; VidFileHeader::get_header_size()];
        channel_in
            .read_exact(&mut hdr_buf)
            .await
            .or_else(|_| Err(TranscodeError::FileTooShort))?;
        let mut cur = Cursor::new(hdr_buf);
        let (v, _sz) = VidFileHeader::open_from_reader(&mut cur)?;
        let hdr = MetaHeader::from_vid_header(v)?;
        Ok((hdr, channel_in))
    }

    // This represents a "reqwest" producer/consumer pair
    async fn transcode_http(
        &mut self,
        channel_in: String,  // URL
        channel_out: String, // URL
        out_is_post: bool,   // post else put TODO - should be an enum
    ) -> Result<(u32, u32), TranscodeError> {
        let jobid = self.jobid;
        println!("job:{jobid} started parallel");
        let st = Instant::now();
        let num_workers = self.num_workers as usize;

        let cli = Client::builder()
            .build()
            .or_else(|_| Err(TranscodeError::Unsupported))?;
        let mut in_reqwest_resp = cli
            .get(channel_in)
            .send()
            .await
            .or_else(|_| Err(TranscodeError::Unsupported))?;

        let Ok(Some(mut hdr_bytes)) = in_reqwest_resp.chunk().await else {
            return Err(TranscodeError::InvalidHeader);
        };
        let (meta_header, mut frame_first_bytes) = {
            let hdr_sz = VidFileHeader::get_header_size();
            let frame_bytes = hdr_bytes.split_off(hdr_sz);
            let hdr_slice = hdr_bytes.as_bytes();
            let (vid_header, _read_tail_buf) = VidFileHeader::open_from_buf(hdr_slice)?;
            let metadata_header = MetaHeader::from_vid_header(vid_header)?;
            (metadata_header, frame_bytes)
        };

        let num_frames = meta_header.num_frames;
        let debug = self.debug;
        let collect_stats = self.collect_stats;
        let rules = self.rules.take().ok_or(TranscodeError::Unsupported)?;
        let frame_read_size = meta_header.frame_size;
        let rules = rules.build(meta_header)?;
        let frame_write_size = rules.get_tgt_dim().calc_size();
        let mut in_buf_stack = WBytesStack::new(frame_read_size);
        let mut out_buf_stack = RBytesStack::new(frame_write_size);
        println!("step1");

        let (wtx, wrx) =
            tokio::sync::mpsc::channel::<Option<(bool, u32, WBytes, RBytes)>>(num_workers);
        let (ctx, crx) = crossbeam_channel::bounded::<(
            u32,
            WBytes,
            RBytes,
            tokio::sync::mpsc::Sender<Option<(bool, u32, WBytes, RBytes)>>,
        )>(num_workers);
        let (rtx, mut rrx) = tokio::sync::mpsc::channel::<(WBytes, RBytes)>(num_workers);

        // prepend the output stream with the frame header
        {
            let tgt_frame_header = VidFileHeader {
                dim: rules.get_tgt_dim(),
                num_frames: meta_header.num_frames,
            };
            let hdr_buf = tgt_frame_header.create_header();
            let hdr_bytes = WBytes::from(hdr_buf);
            let _ = wtx
                .send(Some((true, 0u32, hdr_bytes, RBytes::empty())))
                .await;
        }

        // Spawn CPU processors in dedicated Rayon thread-pool
        let rule_arc = Arc::new(rules);
        let mut cpu_futs = vec![];
        for worker_id in 0..num_workers {
            let crx_ = crx.clone();
            let rules_ = rule_arc.clone();
            let t = tokio::task::spawn_blocking(move || {
                if debug {
                    println!("job: {jobid} worker {worker_id} CPU started");
                }
                let mut min_time = u128::MAX;
                let mut max_time = 0;
                let mut sum_time = 0;
                let mut num_processed = 0;
                let mut last_sent = None;

                while let Ok((fr, in_fr, mut out_fr, nxt)) = crx_.recv() {
                    let st = if debug || collect_stats {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    let in_fr_buf = in_fr.0.as_ref();
                    let mut out_fr_buf = out_fr.0.to_vec();
                    let Ok(_) = rules_.process_frame(fr, in_fr_buf, &mut out_fr_buf) else {
                        println!("bad");
                        return;
                    };
                    out_fr = out_fr_buf.into();
                    num_processed += 1;
                    st.map(|v| {
                        let took = Instant::now().duration_since(v).as_millis();
                        min_time = min_time.min(took);
                        max_time = max_time.max(took);
                        sum_time += took;
                        if debug {
                            println!(
                                "job:{jobid} worker {worker_id} fr:{fr} worker: took:{}",
                                Instant::now().duration_since(v).as_millis()
                            );
                        }
                    });
                    let Ok(_) = nxt.blocking_send(Some((false, fr, in_fr, out_fr))) else {
                        println!("bad");
                        return;
                    };
                    last_sent = Some(nxt);
                }
                let avg = sum_time as f32 / num_processed as f32;
                if debug || collect_stats {
                    println!("job: {jobid} worker {worker_id} CPU ended; min:{min_time} max:{max_time} avg:{avg}");
                }
                if let Some(last_sent) = last_sent {
                    let _ = last_sent.blocking_send(None).unwrap();
                }
            });
            cpu_futs.push(t);
        }
        println!("step2 starting writer");

        // Spawn BG writer (in tokio thread)
        let writer_fut: tokio::task::JoinHandle<Result<u32, TranscodeError>> =
            tokio::spawn(async move {
                let out_cell = Arc::new(AtomicU32::new(0));
                let stream_writer = Self::mk_stream(
                    num_workers as u32,
                    jobid.clone(),
                    debug,
                    num_frames,
                    wrx,
                    rtx,
                    out_cell.clone(),
                );

                let body_stream_wrapped = Body::wrap_stream(stream_writer);

                let r = if out_is_post {
                    cli.post(channel_out)
                } else {
                    cli.put(channel_out)
                };
                println!("step2  writer starting");
                let _item = r // TODO
                    .body(body_stream_wrapped)
                    .send()
                    .await
                    .map_err(|_| TranscodeError::OutputTruncated)?
                    .text()
                    .await
                    .map_err(|_| TranscodeError::OutputTruncated)?;
                println!("step2  writer finished");
                Ok(out_cell.load(SeqCst))
            });

        println!("step3  reader starting");
        // Main thread reader
        {
            let mut num_outstanding = 0;
            // Given that this is an async reader, we assume the input header is already consumed
            for fr in 0..num_frames {
                if debug {
                    println!("main(rdr) creating {fr}");
                };
                if num_outstanding >= num_workers {
                    if let Some((w, r)) = rrx.recv().await {
                        // back pressure
                        in_buf_stack.push(w);
                        out_buf_stack.push(r);
                    }
                }
                num_outstanding += 1;

                let mut in_bytes = in_buf_stack.pop();
                let mut in_buf = in_bytes.0.to_vec();
                let mut to_read = frame_read_size;
                while to_read > 0 {
                    if frame_first_bytes.is_empty() {
                        let Ok(Some(hdr_bytes)) = in_reqwest_resp.chunk().await else {
                            return Err(TranscodeError::FileTooShort);
                        };
                        frame_first_bytes = hdr_bytes;
                    }
                    if frame_first_bytes.len() <= to_read {
                        // partial or filled buffer
                        let to_skip = frame_read_size - to_read;
                        let to_copy = frame_first_bytes.len();
                        let tmp_buf = &mut in_buf[to_skip..to_skip + to_copy];
                        tmp_buf.copy_from_slice(&frame_first_bytes);
                        frame_first_bytes.clear();
                        to_read -= to_copy;
                    } else {
                        // more than we need buffer
                        let cpy_buf = frame_first_bytes.split_to(to_read);
                        let to_skip = frame_read_size - to_read;
                        let tmp_buf = &mut in_buf[to_skip..];
                        tmp_buf.copy_from_slice(&cpy_buf);
                        to_read = 0;
                    }
                }
                in_bytes = in_buf.into();
                let out_buf = out_buf_stack.pop();
                let _ = ctx.send((fr, in_bytes, out_buf, wtx.clone())).unwrap();
            }
            drop(ctx); // shuts down writer to CPU
            println!("step4  reader done");
        }
        let num_frames_written = writer_fut.await.unwrap()?;
        if self.debug || self.collect_stats {
            println!(
                "job:{jobid} finished took: {}ms",
                Instant::now().duration_since(st).as_millis()
            );
        }

        for cpu_fut in cpu_futs.into_iter() {
            let _ = cpu_fut.await.unwrap();
        }
        println!("step5  cpu futs done");

        Ok((num_frames_written, num_frames))
    }

    // This generates an async iterator stream that is compatible with reqwest Body POST writer
    // It uses the tokio (future_utils) stream macro (e.g. the "yield" keyword)
    fn mk_stream(
        num_workers: u32,
        jobid: Ulid,
        debug: bool,
        num_frames: u32,
        mut wrx: tokio::sync::mpsc::Receiver<Option<(bool, u32, WBytes, RBytes)>>,
        rtx: tokio::sync::mpsc::Sender<(WBytes, RBytes)>,
        out_num_frames_written: Arc<AtomicU32>,
    ) -> impl Stream<Item = std::io::Result<Bytes>> {
        try_stream! {
            let mut num_written = 0;
            let mut num_exited = 0;
            let mut reorder: Vec<(u32,WBytes,RBytes)> = vec![]; // used to handle out of order inbounds
            while let Some(msg) = wrx.recv().await {
                if let Some((is_hdr, mut fr, mut wr_bytes, mut rd_bytes)) = msg {
                    if is_hdr { // allows header as first payload
                        yield rd_bytes.0;
                        continue;
                    }
                    loop {
                            if num_written != fr {
                                if debug {
                                    println!("job: {jobid} fr: {fr} writer: out of order");
                                }
                                reorder.push((fr, wr_bytes, rd_bytes));
                                reorder.sort_by(|a, b| a.0.cmp(&b.0));
                                break;
                            } else {
                                // reorder
                                let rd_bytes_clone = rd_bytes.0.clone();
                                yield rd_bytes_clone;
                                num_written += 1;
                                if let Err(_) = rtx.send((wr_bytes, rd_bytes)).await {
                                    // hopefully near end of stream
                                    println!("job: {jobid} fr: {fr} writer: couldnt send to reader; probably exiting");
                                }
                                if !reorder.is_empty() && reorder[0].0 == num_written {
                                    let (fr_, in_bytes_, out_bytes_) = reorder.remove(0);
                                    fr = fr_;
                                    wr_bytes = in_bytes_;
                                    rd_bytes = out_bytes_;
                                    if debug {
                                        println!("job: {jobid} fr: {fr} writer: re-ordering");
                                    }
                                } else {
                                    break;
                                }
                                if num_written == num_frames {
                                    if debug {
                                        println!("job: {jobid} writer: done; num_written: {num_written}");
                                    }
                                    out_num_frames_written.store(num_written, SeqCst);
                                    return;
                                }
                            }
                        }
                } else {
                    num_exited += 1;
                    if num_exited == num_workers {
                        println!("job: {jobid} writer: early exit detected; all workers done; exiting; num_written:{num_written}");
                        out_num_frames_written.store(num_written, SeqCst);
                        return;
                    } else {
                        println!("job: {jobid} writer: early exit detected");
                    }
                }
            }
            out_num_frames_written.store(num_written, SeqCst);
            if debug {
                println!("job: {jobid} writer: iter-exiting");
            }
        }
    }

    // This bridges the gap between non-async and async-tokio by dispatching to a tokio-runtime
    pub(crate) fn transcode_handles_blocking(
        &mut self,
        channel_in: InHandle,
        channel_out: OutHandle,
    ) -> Result<(u32, u32), TranscodeError> {
        match Handle::try_current() {
            Ok(r) => r.block_on(async { self.transcode_handles(channel_in, channel_out).await }),
            Err(_) => tokio::runtime::Builder::new_multi_thread()
                .build()
                .unwrap()
                .block_on(async { self.transcode_handles(channel_in, channel_out).await }),
        }
    }

    // This maps to either a file / stream / or HTTP based handler
    pub(crate) async fn transcode_handles(
        &mut self,
        channel_in: InHandle,
        channel_out: OutHandle,
    ) -> Result<(u32, u32), TranscodeError> {
        match (channel_in, channel_out) {
            (InHandle::HTTP_GET(file_in), OutHandle::HTTP_POST(file_out)) => {
                self.transcode_http(file_in, file_out, true).await
            }
            (InHandle::HTTP_GET(file_in), OutHandle::HTTP_PUT(file_out)) => {
                self.transcode_http(file_in, file_out, false).await
            }
            (InHandle::File(file_in), OutHandle::File(file_out)) => {
                let in_f = tokio::fs::File::open(&file_in)
                    .await
                    .or_else(|_| Err(TranscodeError::FileTooShort))?;
                let out_f = tokio::fs::File::create(&file_out)
                    .await
                    .or_else(|_| Err(TranscodeError::FileTooShort))?;
                self.transcode_channels(in_f, out_f).await
            }
            (InHandle::Stdin, OutHandle::File(file_out)) => {
                let in_f = tokio::io::stdin();
                let out_f = tokio::fs::File::create(&file_out)
                    .await
                    .or_else(|_| Err(TranscodeError::FileTooShort))?;
                self.transcode_channels(in_f, out_f).await
            }
            (InHandle::File(file_in), OutHandle::Stdout) => {
                let in_f = tokio::fs::File::open(&file_in)
                    .await
                    .or_else(|_| Err(TranscodeError::FileTooShort))?;
                let out_f = tokio::io::stdout();
                self.transcode_channels(in_f, out_f).await
            }
            (InHandle::Stdin, OutHandle::Stdout) => {
                let in_f = tokio::io::stdin();
                let out_f = tokio::io::stdout();
                self.transcode_channels(in_f, out_f).await
            }
            _ => Err(TranscodeError::Unsupported),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_flow_st() {
    test_flow_xx(1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 32)]
async fn test_flow_mt() {
    test_flow_xx(4).await;
}

pub(super) async fn test_flow_xx(num_workers: u16) {
    let in_f = File::open("input/video.rvid").await.unwrap();
    let rules = RuleOpts::default()
        .with_json_file_async("input/rules.json")
        .await
        .unwrap();
    // let rules = RuleOpts::default().parse_rules_rdr(meta_header, rule_cursor).unwrap();

    let out_f = File::create("/opt/tmp/foo.rvid").await.unwrap();

    let mut xcoder = TokioTranscoder {
        num_workers,
        rules: Some(rules),
        debug: false,
        jobid: Ulid::default(),
        collect_stats: true,
    };
    create_tp(&mut xcoder);
    let st = Instant::now();
    xcoder.transcode_channels(in_f, out_f).await.unwrap();
    println!("took: {}ms", Instant::now().duration_since(st).as_millis());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_flow_null_out_st() {
    test_flow_null_out_xx(1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_flow_null_out_mt() {
    test_flow_null_out_xx(4).await;
}

#[allow(dead_code)]
async fn test_flow_null_out_xx(num_workers: u16) {
    let mut in_f = File::open("input/video.rvid").await.unwrap();
    let to_read = VidFileHeader::get_header_size();
    let mut hdr_buf = vec![0u8; to_read];
    in_f.read_exact(&mut hdr_buf).await.unwrap();

    let rules = RuleOpts::default()
        .with_json_file_async("input/rules.json")
        .await
        .unwrap();

    let out_f = File::options().write(true).open("/dev/null").await.unwrap();

    let mut xcoder = TokioTranscoder {
        num_workers,
        rules: Some(rules),
        debug: true,
        jobid: Ulid::default(),
        collect_stats: true,
    };
    create_tp(&mut xcoder);
    let st = Instant::now();
    xcoder.transcode_channels(in_f, out_f).await.unwrap();
    println!("took: {}ms", Instant::now().duration_since(st).as_millis());
}

fn create_tp(_w: &mut TokioTranscoder) {}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_flow_null_in_out_st() {
    test_flow_null_in_out_x(1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_flow_null_in_out_mt() {
    test_flow_null_in_out_x(4).await;
}

#[allow(dead_code)]
pub(super) async fn test_flow_null_in_out_x(num_workers: u16) {
    // let mut in_f = File::open("input/video.rvid").await.unwrap();
    // let to_read = VidFileHeader::get_header_size();
    // let mut hdr_buf = vec![0u8; to_read];
    // in_f.read_exact(&mut hdr_buf).await.unwrap();
    // let mut hdr_cursor = Cursor::new(hdr_buf);
    // let (vid_hdr, _) = VidFileHeader::open_from_reader(&mut hdr_cursor).unwrap();
    // drop(in_f);
    let in_f = File::open("/dev/zero").await.unwrap();

    let rules = RuleOpts::default()
        .with_json_file_async("input/rules.json")
        .await
        .unwrap();

    let out_f = File::options().write(true).open("/dev/null").await.unwrap();

    let mut xcoder = TokioTranscoder {
        num_workers,
        rules: Some(rules),
        debug: false,
        jobid: Ulid::default(),
        collect_stats: true,
    };
    create_tp(&mut xcoder);

    let st = Instant::now();
    // TODO broken!!!!
    xcoder.transcode_channels(in_f, out_f).await.unwrap();
    println!("took: {}ms", Instant::now().duration_since(st).as_millis());
}
