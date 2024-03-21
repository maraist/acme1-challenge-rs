use std::io::{IoSlice, IoSliceMut};
use std::sync::Arc;
use std::time::Instant;

use crate::data::{InHandle, OutHandle};
use futures_lite::AsyncReadExt;
use glommio::channels::shared_channel;
use glommio::io::{DmaFile, DmaStreamReaderBuilder, DmaStreamWriterBuilder};
use glommio::{LocalExecutorBuilder, Placement};

use crate::meta::MetaHeader;
use crate::rules::{PartialRules, Rules};
use crate::spec::VidFileHeader;
use crate::transcoder::bufstack::BufStack;
use crate::TranscodeError;

pub struct GlommioTranscoder {
    pub num_workers: u16,
    // pub meta_header: MetaHeader,
    pub rules: Option<PartialRules>,
}

fn proc(src: &[u8], dst: &mut [u8]) {
    if false {
        for _ in 0..6 {
            for (a, b) in src.iter().zip(dst.iter_mut()) {
                *b = b.wrapping_add(1).wrapping_add(*a);
            }
        }
    } else {
        let mut r = 0;
        while r < 2160 {
            r += 70;
            let mut c = 0;
            while c < 3600 {
                c += 50;
                for r1 in r..r + 16 {
                    let s = r1 * 9000 + c * 3;
                    let e = s + 16;
                    for (a, b) in (&src[s..e]).iter().zip((&mut dst[s..e]).iter_mut()) {
                        *b = b.wrapping_add(1).wrapping_add(*a);
                    }
                }
            }
        }
    }
}

#[test]
fn test_glommio() {
    run_glommio();
}
pub(super) fn run_glommio() {
    const MODE: u8 = 1;
    if MODE == 0 {
        run_glommio_st();
    } else {
        run_glommio_mt();
    }
}

// This is here so I can perform a command line tool to test this workflow
pub fn run_glommio_st() {
    const SZ: usize = 3840 * 2160 * 3;
    const DEBUG: bool = false;

    let st = Instant::now();
    {
        let b = LocalExecutorBuilder::new(Placement::Fixed(0))
            .spawn(|| async move {
                use futures_lite::io::AsyncWriteExt;
                let out_f = DmaFile::create("/opt/tmp/foo.dma").await.unwrap();
                let mut writer = DmaStreamWriterBuilder::new(out_f)
                    .with_buffer_size(SZ)
                    .build();

                let in_f = DmaFile::open("input/video.rvid").await.unwrap();
                let mut reader = DmaStreamReaderBuilder::new(in_f)
                    .with_buffer_size(SZ)
                    .build();

                let mut proc_stack: Vec<(u32, Vec<u8>)> = vec![];
                let mut out_stack: Vec<(u32, Vec<u8>)> = vec![];

                let mut free_stack = BufStack::new(SZ);
                let num_resends = 0;
                let mut num_fgs = 0;
                for fr in 0..600 {
                    if fr == 0 {
                        let mut left = free_stack.pop();
                        let _ = reader.read_exact(&mut left).await.unwrap();
                        let buf_l = free_stack.pop();
                        let mut out_l = free_stack.pop();
                        proc(&buf_l, &mut out_l);
                        out_stack.push((0, out_l));
                        free_stack.push(buf_l);
                    } else if fr == 1 {
                        let mut left = free_stack.pop();
                        let _ = reader.read_exact(&mut left).await.unwrap();
                        proc_stack.push((1, left));
                    } else {
                        let (fr_out_l, out_l) = out_stack.pop().unwrap();
                        let wr_f = writer.write_all(&out_l);
                        if DEBUG {
                            println!("wr {fr_out_l}");
                        }

                        let mut in_l = free_stack.pop();
                        let rd_f = reader.read_exact(&mut in_l);

                        while !proc_stack.is_empty() {
                            let (frr, src) = proc_stack.pop().unwrap();
                            let mut tgt = free_stack.pop();
                            proc(&src, &mut tgt);
                            free_stack.push(src);
                            out_stack.push((frr, tgt));
                        }

                        let _ = rd_f.await.unwrap();
                        let _ = wr_f.await.unwrap();

                        free_stack.push(out_l);
                        proc_stack.push((fr, in_l));
                    }
                }
                println!("end input");

                // reads are done
                while let Some((fr, proc_data)) = proc_stack.pop() {
                    let mut out = free_stack.pop();
                    proc(&proc_data, &mut out);
                    num_fgs += 1;
                    if DEBUG {
                        println!("wr {fr}");
                    }
                    let _ = writer.write_all(&out).await;
                }
                println!("proc-stack done");

                while let Some((ffl, left)) = out_stack.pop() {
                    if DEBUG {
                        println!("wr {ffl} ");
                    }
                    let _ = writer.write_all(&left).await.unwrap();
                }
                println!("all done num_resends:{num_resends} num_fgs:{num_fgs}");
            })
            .unwrap();
        let _ = b.join();
    }
    let ed = Instant::now();
    println!("took {}ms", ed.duration_since(st).as_millis());
}

// This is here so I can perform a command line tool to test this workflow
pub fn run_glommio_mt() {
    const SZ: usize = 3840 * 2160 * 3;
    const DEBUG: bool = false;
    const MODE: u8 = 1;

    let st = Instant::now();
    {
        let (m2w_snd, m2w_rcv) = shared_channel::new_bounded::<(u32, Vec<u8>, Vec<u8>)>(4);
        let (w2m_snd, w2m_rcv) = shared_channel::new_bounded::<(u32, Vec<u8>, Vec<u8>)>(4);
        let task = LocalExecutorBuilder::new(Placement::Fixed(0))
            .spawn(|| async move {
                let m2w_rcv = m2w_rcv.connect().await;
                let w2m_snd = w2m_snd.connect().await;
                while let Some((fr, src, mut dst)) = m2w_rcv.recv().await {
                    proc(&src, &mut dst);
                    w2m_snd.send((fr, src, dst)).await.unwrap();
                }
            })
            .unwrap();

        let b = LocalExecutorBuilder::new(Placement::Fixed(0))
            .spawn(|| async move {
                let m2w_snd = m2w_snd.connect().await;
                let w2m_rcv = w2m_rcv.connect().await;

                use futures_lite::io::AsyncWriteExt;
                let out_f = DmaFile::create("/opt/tmp/foo.dma").await.unwrap();
                let mut writer = DmaStreamWriterBuilder::new(out_f)
                    .with_buffer_size(2 * SZ)
                    .build();

                let in_f = DmaFile::open("input/video.rvid").await.unwrap();
                let mut reader = DmaStreamReaderBuilder::new(in_f)
                    .with_buffer_size(2 * SZ)
                    .build();

                let mut proc_stack: Vec<(u32, Vec<u8>)> = vec![];
                let mut out_stack: Vec<((u32, Vec<u8>), (u32, Vec<u8>))> = vec![];

                let mut free_stack = BufStack::new(SZ);

                let mut proc_done = Vec::with_capacity(2);

                let mut fr = 0;
                let mut num_resends = 0;
                let mut num_fgs = 0;
                while fr < 600 {
                    if fr == 0 {
                        // rd 2, submit-2-proc
                        let mut left = free_stack.pop();
                        let mut right = free_stack.pop();
                        let mut offset = 0usize;
                        let len_ = left.len() + right.len();
                        loop {
                            let mut lr = [IoSliceMut::new(&mut left), IoSliceMut::new(&mut right)];
                            let mut lr_buf = &mut lr[..];
                            IoSliceMut::advance_slices(&mut lr_buf, offset);
                            let len = reader.read_vectored(lr_buf).await.unwrap();
                            offset += len;
                            if offset == len_ {
                                break;
                            }
                            if len == 0 {
                                panic!("eof");
                            }
                            if offset > len_ {
                                panic!("adv too far {offset} len:{len} len_:{len_}");
                            }
                            num_resends += 1;
                            // println!("1 pass off:{offset} len:{len} len_:{len_}");
                        }
                        let buf_l = free_stack.pop();
                        let buf_r = free_stack.pop();
                        let _ = m2w_snd.send((0, left, buf_l)).await.unwrap();
                        let _ = m2w_snd.send((1, right, buf_r)).await.unwrap();
                    } else if fr == 2 {
                        // rd 2, sub-1-proc, proc-1-locally, schedule out-2
                        let mut left = free_stack.pop();
                        let mut right = free_stack.pop();
                        let mut offset = 0usize;
                        let len_ = left.len() + right.len();
                        loop {
                            let mut lr = [IoSliceMut::new(&mut left), IoSliceMut::new(&mut right)];
                            let mut lr_buf = &mut lr[..];
                            IoSliceMut::advance_slices(&mut lr_buf, offset);
                            let len = reader.read_vectored(lr_buf).await.unwrap();
                            offset += len;
                            if offset == len_ {
                                break;
                            }
                            if len == 0 {
                                panic!("eof");
                            }
                            if offset > len_ {
                                panic!("adv too far");
                            }
                            num_resends += 1;
                            // println!("2 pass off:{offset} len:{len} len_:{len_}");
                        }
                        let mut buf_r = free_stack.pop();
                        if MODE == 0 {
                            proc_stack.push((1, left));
                        } else {
                            let buf_l = free_stack.pop();
                            let _ = m2w_snd.send((2, left, buf_l)).await.unwrap();
                        }
                        proc(&right, &mut buf_r);
                        num_fgs += 1;
                        let (ff, old_l_rd, old_l_proc) = w2m_rcv.recv().await.unwrap();
                        free_stack.push(right);
                        free_stack.push(old_l_rd);
                        out_stack.push(((3, buf_r), (ff, old_l_proc)));
                    } else {
                        let ((fr_out_l, out_l), (fr_in_r, out_r)) = out_stack.pop().unwrap();
                        let out_lr = [IoSlice::new(&out_l), IoSlice::new(&out_r)];
                        if DEBUG {
                            println!("wr {fr_out_l} , {fr_in_r}");
                        }
                        let wr_f = writer.write_vectored(&out_lr);
                        let mut in_l = free_stack.pop();
                        let mut in_r = free_stack.pop();

                        let mut in_lr = [IoSliceMut::new(&mut in_l), IoSliceMut::new(&mut in_r)];
                        let rd_f = reader.read_vectored(&mut in_lr);
                        let fr_in_l = fr;
                        let fr_in_r = fr + 1;

                        if MODE == 0 {
                            // skip
                        } else {
                            while proc_stack.len() > 1 {
                                let (frr, src) = proc_stack.pop().unwrap();
                                let tgt = free_stack.pop();
                                m2w_snd.send((frr, src, tgt)).await.unwrap();
                            }
                        }
                        while !proc_stack.is_empty() {
                            let (frr, src) = proc_stack.pop().unwrap();
                            let mut tgt = free_stack.pop();
                            proc(&src, &mut tgt);
                            num_fgs += 1;
                            free_stack.push(src);
                            proc_done.push((frr, tgt));
                        }
                        while proc_done.len() < 2 {
                            let (frr, src, out) = w2m_rcv.recv().await.unwrap();
                            free_stack.push(src);
                            proc_done.push((frr, out));
                        }
                        out_stack.push((proc_done.pop().unwrap(), proc_done.pop().unwrap()));
                        let len = rd_f.await.unwrap();

                        let mut offset = len;
                        let len_ = in_l.len() + in_r.len();
                        while offset != len_ {
                            num_resends += 1;
                            // println!("3rd end pass off:{offset} len:{len} len_:{len_}");
                            let mut lr = [IoSliceMut::new(&mut in_l), IoSliceMut::new(&mut in_r)];
                            let mut lr_buf = &mut lr[..];
                            IoSliceMut::advance_slices(&mut lr_buf, offset);
                            let len = reader.read_vectored(lr_buf).await.unwrap();
                            offset += len;
                            if offset > len_ {
                                panic!("3rd adv too far offset:{offset} len:{len} len_:{len_}");
                            }
                            if len == 0 {
                                panic!("eof");
                            }
                            // println!("3rd pass off:{offset} len:{len} len_:{len_}");
                        }

                        let len = wr_f.await.unwrap();
                        let mut offset = len;

                        let len_ = out_l.len() + out_r.len();
                        while offset != len_ {
                            num_resends += 1;
                            // println!("4wr end pass off:{offset} len:{len} len_:{len_}");
                            let mut lr = [IoSlice::new(&out_l), IoSlice::new(&out_r)];
                            let mut lr_buf = &mut lr[..];
                            IoSlice::advance_slices(&mut lr_buf, offset);
                            let len = writer.write_vectored(lr_buf).await.unwrap();
                            offset += len;
                            if offset > len_ {
                                panic!("4wr adv too far");
                            }
                            if len == 0 {
                                panic!("eof");
                            }
                            // println!("4wr pass off:{offset} len:{len} len_:{len_}");
                        }

                        free_stack.push(out_l);
                        free_stack.push(out_r);
                        proc_stack.push((fr_in_l, in_l));
                        proc_stack.push((fr_in_r, in_r));
                    }
                    // if debug {
                    //     println!("end frame {fr}");
                    // }
                    fr += 2;
                }
                drop(m2w_snd);
                println!("end input");

                // reads are done
                while let Some((fr, proc_data)) = proc_stack.pop() {
                    let mut out = free_stack.pop();
                    proc(&proc_data, &mut out);
                    num_fgs += 1;
                    if DEBUG {
                        println!("wr {fr}");
                    }
                    let _ = writer.write_all(&out).await;
                }
                println!("proc-stack done");
                while let Some((fr, _buf, proc_data)) = w2m_rcv.recv().await {
                    if DEBUG {
                        println!("wr {fr}");
                    }
                    let _ = writer.write_all(&proc_data).await;
                }
                println!("workers are done");

                while let Some(((ffl, left), (ffr, right))) = out_stack.pop() {
                    // let _ = writer.write_vectored(&[l,r]).await;   let mut offset = 0usize;
                    if DEBUG {
                        println!("wr {ffl} , {ffr}");
                    }
                    let mut offset = 0usize;
                    let len_ = left.len() + right.len();
                    loop {
                        let mut lr = [IoSlice::new(&left), IoSlice::new(&right)];
                        let mut lr_buf = &mut lr[..];
                        IoSlice::advance_slices(&mut lr_buf, offset);
                        let len = writer.write_vectored(&lr_buf).await.unwrap();
                        offset += len;
                        if offset == len_ {
                            break;
                        }
                        if offset > len_ {
                            panic!("adv too far");
                        }
                        if len == 0 {
                            panic!("eof");
                        }
                        num_resends += 1;
                        // println!("5 pass off:{offset} len:{len} len_:{len_}");
                    }
                }
                println!("all done num_resends:{num_resends} num_fgs:{num_fgs}");
            })
            .unwrap();
        let _ = task.join();
        let _ = b.join();
    }
    let ed = Instant::now();
    println!("took {}ms", ed.duration_since(st).as_millis());
}

impl GlommioTranscoder {
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
            file_name_in.as_str(),
            self.rules.take().unwrap(),
        )?;

        // write header
        let tgt_frame_header = VidFileHeader {
            dim: rules.get_tgt_dim(),
            num_frames: meta_header.num_frames,
        };

        let src_sz: usize = meta_header.frame_size;
        let tgt_sz: usize = rules.get_tgt_dim().calc_size();
        let num_frames = meta_header.num_frames;

        let st = Instant::now();
        {
            if self.num_workers > 1 {
                Self::do_glommio_mt(
                    file_name_in,
                    file_name_out,
                    rules,
                    tgt_frame_header,
                    src_sz,
                    tgt_sz,
                    num_frames,
                );
            } else {
                Self::do_glommio_st(
                    file_name_in,
                    file_name_out,
                    rules,
                    tgt_frame_header,
                    src_sz,
                    tgt_sz,
                    num_frames,
                );
            }
        }
        let ed = Instant::now();
        println!("took {}ms", ed.duration_since(st).as_millis());

        Ok(())
    }

    fn do_glommio_st(
        file_name_in: String,
        file_name_out: String,
        rules: Rules,
        tgt_frame_header: VidFileHeader,
        src_sz: usize,
        tgt_sz: usize,
        num_frames: u32,
    ) {
        const DEBUG: bool = false;
        let b = LocalExecutorBuilder::new(Placement::Fixed(0))
            .spawn(move || async move {
                use futures_lite::io::AsyncWriteExt;
                let out_f = DmaFile::create(file_name_out).await.unwrap();
                let mut writer = DmaStreamWriterBuilder::new(out_f)
                    .with_buffer_size(src_sz)
                    .build();
                {
                    let hdr_buf = tgt_frame_header.create_header();
                    let _ = writer.write_all(hdr_buf.as_slice()).await.unwrap();
                }

                let in_f = DmaFile::open(file_name_in).await.unwrap();
                let mut reader = DmaStreamReaderBuilder::new(in_f)
                    .with_buffer_size(tgt_sz)
                    .build();

                let mut proc_stack: Vec<(u32, Vec<u8>)> = vec![];
                let mut out_stack: Vec<(u32, Vec<u8>)> = vec![];

                let mut free_src_stack = BufStack::new(src_sz);
                let mut free_tgt_stack = BufStack::new(tgt_sz);
                for fr in 0..num_frames {
                    if fr == 0 {
                        let mut left = free_src_stack.pop();
                        let _ = reader.read_exact(&mut left).await.unwrap();
                        let mut out_l = free_tgt_stack.pop();
                        rules
                            .process_frame(0, left.as_slice(), out_l.as_mut_slice())
                            .unwrap();
                        out_stack.push((0, out_l));
                        free_src_stack.push(left);
                    } else if fr == 1 {
                        let mut left = free_src_stack.pop();
                        let _ = reader.read_exact(&mut left).await.unwrap();
                        proc_stack.push((1, left));
                    } else {
                        let (fr_out_l, tgt_wr_buf) = out_stack.pop().unwrap();
                        let wr_f = writer.write_all(&tgt_wr_buf);
                        if DEBUG {
                            println!("wr {fr_out_l}");
                        }

                        let mut src_rd_buf = free_src_stack.pop();
                        let rd_f = reader.read_exact(&mut src_rd_buf);

                        while !proc_stack.is_empty() {
                            let (frr, src) = proc_stack.pop().unwrap();
                            let mut tgt = free_tgt_stack.pop();
                            rules
                                .process_frame(frr, src.as_slice(), tgt.as_mut_slice())
                                .unwrap();
                            proc(&src, &mut tgt);
                            free_src_stack.push(src);
                            out_stack.push((frr, tgt));
                        }

                        let _ = rd_f.await.unwrap();
                        let _ = wr_f.await.unwrap();

                        free_tgt_stack.push(tgt_wr_buf);
                        proc_stack.push((fr, src_rd_buf));
                    }
                }
                println!("end input");

                // reads are done
                while let Some((fr, proc_data)) = proc_stack.pop() {
                    let mut out = free_tgt_stack.pop();
                    rules
                        .process_frame(fr, proc_data.as_slice(), out.as_mut_slice())
                        .unwrap();
                    if DEBUG {
                        println!("wr {fr}");
                    }
                    let _ = writer.write_all(&out).await;
                }
                println!("proc-stack done");

                while let Some((ffl, left)) = out_stack.pop() {
                    if DEBUG {
                        println!("wr {ffl} ");
                    }
                    let _ = writer.write_all(&left).await.unwrap();
                }
            })
            .unwrap();
        let _ = b.join();
    }

    fn do_glommio_mt(
        _file_name_in: String,
        _file_name_out: String,
        rules: Rules,
        tgt_frame_header: VidFileHeader,
        src_sz: usize,
        tgt_sz: usize,
        _num_frames: u32,
    ) {
        // const SZ:usize  = 3840 * 2160 * 3;
        const DEBUG: bool = false;
        const MODE: u8 = 1;
        let rules = Arc::new(rules);

        let st = Instant::now();
        {
            let (m2w_snd, m2w_rcv) = shared_channel::new_bounded::<(u32, Vec<u8>, Vec<u8>)>(4);
            let (w2m_snd, w2m_rcv) = shared_channel::new_bounded::<(u32, Vec<u8>, Vec<u8>)>(4);
            let rules_ = rules.clone();
            let task = LocalExecutorBuilder::new(Placement::Fixed(1))
                .spawn(|| async move {
                    let m2w_rcv = m2w_rcv.connect().await;
                    let w2m_snd = w2m_snd.connect().await;
                    while let Some((fr, src, mut dst)) = m2w_rcv.recv().await {
                        rules_
                            .process_frame(fr, src.as_slice(), dst.as_mut_slice())
                            .unwrap();
                        w2m_snd.send((fr, src, dst)).await.unwrap();
                    }
                })
                .unwrap();

            let b = LocalExecutorBuilder::new(Placement::Fixed(2))
                .spawn(move || async move {
                    let m2w_snd = m2w_snd.connect().await;
                    let w2m_rcv = w2m_rcv.connect().await;

                    use futures_lite::io::AsyncWriteExt;
                    let out_f = DmaFile::create("/opt/tmp/foo.dma").await.unwrap();
                    let mut writer = DmaStreamWriterBuilder::new(out_f)
                        .with_buffer_size(2 * tgt_sz)
                        .build();
                    {
                        let hdr_buf = tgt_frame_header.create_header();
                        let _ = writer.write_all(hdr_buf.as_slice()).await.unwrap();
                    }

                    let in_f = DmaFile::open("input/video.rvid").await.unwrap();
                    let mut reader = DmaStreamReaderBuilder::new(in_f)
                        .with_buffer_size(2 * src_sz)
                        .build();

                    let mut proc_stack: Vec<(u32, Vec<u8>)> = vec![];
                    let mut out_stack: Vec<((u32, Vec<u8>), (u32, Vec<u8>))> = vec![];

                    let mut free_src_stack = BufStack::new(src_sz);
                    let mut free_tgt_stack = BufStack::new(tgt_sz);

                    let mut proc_done = Vec::with_capacity(2);

                    let mut fr = 0;
                    let mut num_resends = 0;
                    let mut num_fgs = 0;
                    while fr < tgt_frame_header.num_frames {
                        if fr == 0 {
                            // rd 2, submit-2-proc
                            let mut left = free_src_stack.pop();
                            let mut right = free_src_stack.pop();
                            let mut offset = 0usize;
                            let len_ = left.len() + right.len();
                            loop {
                                let mut lr =
                                    [IoSliceMut::new(&mut left), IoSliceMut::new(&mut right)];
                                let mut lr_buf = &mut lr[..];
                                IoSliceMut::advance_slices(&mut lr_buf, offset);
                                let len = reader.read_vectored(lr_buf).await.unwrap();
                                offset += len;
                                if offset == len_ {
                                    break;
                                }
                                if len == 0 {
                                    panic!("eof");
                                }
                                if offset > len_ {
                                    panic!("adv too far {offset} len:{len} len_:{len_}");
                                }
                                num_resends += 1;
                                // println!("1 pass off:{offset} len:{len} len_:{len_}");
                            }
                            let buf_l = free_tgt_stack.pop();
                            let buf_r = free_tgt_stack.pop();
                            let _ = m2w_snd.send((0, left, buf_l)).await.unwrap();
                            let _ = m2w_snd.send((1, right, buf_r)).await.unwrap();
                        } else if fr == 2 {
                            // rd 2, sub-1-proc, proc-1-locally, schedule out-2
                            let mut left = free_src_stack.pop();
                            let mut right = free_src_stack.pop();
                            let mut offset = 0usize;
                            let len_ = left.len() + right.len();
                            loop {
                                let mut lr =
                                    [IoSliceMut::new(&mut left), IoSliceMut::new(&mut right)];
                                let mut lr_buf = &mut lr[..];
                                IoSliceMut::advance_slices(&mut lr_buf, offset);
                                let len = reader.read_vectored(lr_buf).await.unwrap();
                                offset += len;
                                if offset == len_ {
                                    break;
                                }
                                if len == 0 {
                                    panic!("eof");
                                }
                                if offset > len_ {
                                    panic!("adv too far");
                                }
                                num_resends += 1;
                                // println!("2 pass off:{offset} len:{len} len_:{len_}");
                            }
                            let mut buf_r = free_tgt_stack.pop();
                            if MODE == 0 {
                                proc_stack.push((1, left));
                            } else {
                                let buf_l = free_tgt_stack.pop();
                                let _ = m2w_snd.send((2, left, buf_l)).await.unwrap();
                            }
                            rules
                                .process_frame(2, right.as_slice(), buf_r.as_mut_slice())
                                .unwrap();
                            num_fgs += 1;
                            let (ff, old_l_rd, old_l_proc) = w2m_rcv.recv().await.unwrap();
                            free_src_stack.push(right);
                            free_src_stack.push(old_l_rd);
                            out_stack.push(((3, buf_r), (ff, old_l_proc)));
                        } else {
                            let ((fr_out_l, out_l), (fr_in_r, out_r)) = out_stack.pop().unwrap();
                            let out_lr = [IoSlice::new(&out_l), IoSlice::new(&out_r)];
                            if DEBUG {
                                println!("wr {fr_out_l} , {fr_in_r}");
                            }
                            let wr_f = writer.write_vectored(&out_lr);
                            let mut in_l = free_src_stack.pop();
                            let mut in_r = free_src_stack.pop();

                            let mut in_lr =
                                [IoSliceMut::new(&mut in_l), IoSliceMut::new(&mut in_r)];
                            let rd_f = reader.read_vectored(&mut in_lr);
                            let fr_in_l = fr;
                            let fr_in_r = fr + 1;

                            if MODE == 0 {
                                // skip
                            } else {
                                while proc_stack.len() > 1 {
                                    let (frr, src) = proc_stack.pop().unwrap();
                                    let tgt = free_tgt_stack.pop();
                                    m2w_snd.send((frr, src, tgt)).await.unwrap();
                                }
                            }
                            while !proc_stack.is_empty() {
                                let (frr, src) = proc_stack.pop().unwrap();
                                let mut tgt = free_tgt_stack.pop();
                                rules
                                    .process_frame(frr, src.as_slice(), tgt.as_mut_slice())
                                    .unwrap();
                                num_fgs += 1;
                                free_src_stack.push(src);
                                proc_done.push((frr, tgt));
                            }
                            while proc_done.len() < 2 {
                                let (frr, src, out) = w2m_rcv.recv().await.unwrap();
                                free_src_stack.push(src);
                                proc_done.push((frr, out));
                            }
                            out_stack.push((proc_done.pop().unwrap(), proc_done.pop().unwrap()));
                            let len = rd_f.await.unwrap();

                            let mut offset = len;
                            let len_ = in_l.len() + in_r.len();
                            while offset != len_ {
                                num_resends += 1;
                                // println!("3rd end pass off:{offset} len:{len} len_:{len_}");
                                let mut lr =
                                    [IoSliceMut::new(&mut in_l), IoSliceMut::new(&mut in_r)];
                                let mut lr_buf = &mut lr[..];
                                IoSliceMut::advance_slices(&mut lr_buf, offset);
                                let len = reader.read_vectored(lr_buf).await.unwrap();
                                offset += len;
                                if offset > len_ {
                                    panic!("3rd adv too far offset:{offset} len:{len} len_:{len_}");
                                }
                                if len == 0 {
                                    panic!("eof");
                                }
                                // println!("3rd pass off:{offset} len:{len} len_:{len_}");
                            }

                            let len = wr_f.await.unwrap();
                            let mut offset = len;

                            let len_ = out_l.len() + out_r.len();
                            while offset != len_ {
                                num_resends += 1;
                                // println!("4wr end pass off:{offset} len:{len} len_:{len_}");
                                let mut lr = [IoSlice::new(&out_l), IoSlice::new(&out_r)];
                                let mut lr_buf = &mut lr[..];
                                IoSlice::advance_slices(&mut lr_buf, offset);
                                let len = writer.write_vectored(lr_buf).await.unwrap();
                                offset += len;
                                if offset > len_ {
                                    panic!("4wr adv too far");
                                }
                                if len == 0 {
                                    panic!("eof");
                                }
                                // println!("4wr pass off:{offset} len:{len} len_:{len_}");
                            }

                            free_tgt_stack.push(out_l);
                            free_tgt_stack.push(out_r);
                            proc_stack.push((fr_in_l, in_l));
                            proc_stack.push((fr_in_r, in_r));
                        }
                        // if debug {
                        //     println!("end frame {fr}");
                        // }
                        fr += 2;
                    }
                    drop(m2w_snd);
                    println!("end input");

                    // reads are done
                    while let Some((fr, proc_data)) = proc_stack.pop() {
                        let mut out = free_tgt_stack.pop();
                        rules
                            .process_frame(fr, proc_data.as_slice(), out.as_mut_slice())
                            .unwrap();
                        num_fgs += 1;
                        if DEBUG {
                            println!("wr {fr}");
                        }
                        let _ = writer.write_all(&out).await;
                    }
                    println!("proc-stack done");
                    while let Some((fr, _buf, proc_data)) = w2m_rcv.recv().await {
                        if DEBUG {
                            println!("wr {fr}");
                        }
                        let _ = writer.write_all(&proc_data).await;
                    }
                    println!("workers are done");

                    while let Some(((ffl, left), (ffr, right))) = out_stack.pop() {
                        // let _ = writer.write_vectored(&[l,r]).await;   let mut offset = 0usize;
                        if DEBUG {
                            println!("wr {ffl} , {ffr}");
                        }
                        let mut offset = 0usize;
                        let len_ = left.len() + right.len();
                        loop {
                            let mut lr = [IoSlice::new(&left), IoSlice::new(&right)];
                            let mut lr_buf = &mut lr[..];
                            IoSlice::advance_slices(&mut lr_buf, offset);
                            let len = writer.write_vectored(&lr_buf).await.unwrap();
                            offset += len;
                            if offset == len_ {
                                break;
                            }
                            if offset > len_ {
                                panic!("adv too far");
                            }
                            if len == 0 {
                                panic!("eof");
                            }
                            num_resends += 1;
                            // println!("5 pass off:{offset} len:{len} len_:{len_}");
                        }
                    }
                    println!("all done num_resends:{num_resends} num_fgs:{num_fgs}");
                })
                .unwrap();
            let _ = task.join();
            let _ = b.join();
        }
        let ed = Instant::now();
        println!("took {}ms", ed.duration_since(st).as_millis());
    }
}
