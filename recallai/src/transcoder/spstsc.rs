use std::io::{stdin, stdout, Read, Write};

use crate::data::{InHandle, OutHandle};
use crate::meta::MetaHeader;
use crate::rules::PartialRules;
use crate::spec::VidFileHeader;
use crate::TranscodeError;

// Reference implementation
//
// This makes zero attempt at performance.
// Blocking Read -> CPU processing -> Blocking Write
//
// However, it's the only implementation that works with stdio / stdout
// since it can work with pure streams that don't have seek
pub struct SpStScTranscoder {
    // pub meta_header: MetaHeader,
    pub rules: Option<PartialRules>,
}

impl SpStScTranscoder {
    pub(crate) fn transcode_handles(
        &mut self,
        channel_in: InHandle,
        channel_out: OutHandle,
    ) -> Result<(), TranscodeError> {
        match (channel_in, channel_out) {
            (InHandle::File(file_in), OutHandle::File(file_out)) => {
                let in_f = std::fs::File::open(&file_in)?;
                let out_f = std::fs::File::create(&file_out)?;
                self.transcode(in_f, out_f)
            }
            (InHandle::Stdin, OutHandle::File(file_out)) => {
                let in_f = stdin().lock();
                let out_f = std::fs::File::create(&file_out)?;
                self.transcode(in_f, out_f)
            }
            (InHandle::File(file_in), OutHandle::Stdout) => {
                let in_f = std::fs::File::open(&file_in)?;
                let out_f = stdout().lock();
                self.transcode(in_f, out_f)
            }
            (InHandle::Stdin, OutHandle::Stdout) => {
                let in_f = stdin().lock();
                let out_f = stdout().lock();
                self.transcode(in_f, out_f)
            }
            _ => Err(TranscodeError::Unsupported),
        }
    }

    pub(crate) fn transcode<R, W>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> Result<(), TranscodeError>
    where
        R: Read,
        W: Write,
    {
        let (meta_header, rules) = MetaHeader::from_reader_and_partial_rules(
            &mut reader,
            self.rules.take().ok_or(TranscodeError::Unsupported)?,
        )?;
        let src_capacity = meta_header.frame_size;
        let mut src_buf = vec![0u8; src_capacity].into_boxed_slice();
        let tgt_dim = rules.get_tgt_dim();
        let tgt_capacity = tgt_dim.calc_size();
        let mut tgt_buf = vec![0u8; tgt_capacity].into_boxed_slice();

        let tgt_frame_header = VidFileHeader {
            dim: tgt_dim,
            num_frames: meta_header.num_frames,
        };
        tgt_frame_header.write_header(&mut writer)?;

        // highly inefficient under slow IO, and read and write happen inline with
        // processing.
        // This is only the reference implementation
        for frame in 0..meta_header.num_frames {
            // blocking read
            reader
                .read_exact(&mut src_buf)
                .map_err(TranscodeError::IoErr)?;
            // single threaded processing
            rules.process_frame(frame, &src_buf, &mut tgt_buf)?;
            // blocking write
            writer.write(&tgt_buf).map_err(TranscodeError::IoErr)?;
        }
        Ok(())
    }
}
