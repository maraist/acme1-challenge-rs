use crate::data::Dim;
use crate::meta::MetaHeader;
use crate::util::{emit_u32_be, munch_u32_be};
use crate::TranscodeError;
use std::io::{Read, Write};

// Assuming 3 channels, 1 byte per channel, as per spec
pub(crate) const BYTES_PER_PIXEL: u32 = 3;

// As defined in specification: All Big-Endian
// [width: 4 bytes]
// [height: 4 bytes]
// [total number of frames (n): 4 bytes]
#[derive(Debug, Copy, Clone)]
pub(crate) struct VidFileHeader {
    pub(crate) dim: Dim,
    pub(crate) num_frames: u32,
}

impl From<MetaHeader> for VidFileHeader {
    fn from(peer: MetaHeader) -> Self {
        Self {
            dim: peer.dim,
            num_frames: peer.num_frames,
        }
    }
}

impl VidFileHeader {
    // The Defined header is a fixed length structure
    #[inline]
    pub(crate) const fn get_header_size() -> usize {
        const HEADER_SIZE: usize = std::mem::size_of::<VidFileHeader>();
        HEADER_SIZE
    }
    pub(crate) fn open_from_buf(buf: &[u8]) -> Result<(Self, &[u8]), TranscodeError> {
        const HEADER_SIZE: usize = VidFileHeader::get_header_size();
        if buf.len() < HEADER_SIZE {
            return Err(TranscodeError::FileTooShort);
        }
        let (hdr_buf, _tail) = buf.split_at(HEADER_SIZE);
        let (width, tail) = munch_u32_be(&hdr_buf);
        let (height, tail) = munch_u32_be(&tail);
        let (num_frames, _tail) = munch_u32_be(&tail);
        let dim = Dim { width, height };
        let file_header = VidFileHeader { dim, num_frames };
        Ok((file_header, tail))
    }
    pub(crate) fn open_from_reader<R>(reader: &mut R) -> Result<(Self, usize), TranscodeError>
    where
        R: Read,
    {
        const HEADER_SIZE: usize = VidFileHeader::get_header_size();
        let mut buf = [0u8; HEADER_SIZE];
        let _ = reader.read_exact(&mut buf)?;
        Self::open_from_buf(&buf).map(|(header, _tail)| (header, HEADER_SIZE))
    }

    pub(crate) fn fill_header(&self, buf: &mut [u8]) {
        let mut tail = emit_u32_be(buf, self.dim.width);
        let mut tail = emit_u32_be(&mut tail, self.dim.height);
        let _tail = emit_u32_be(&mut tail, self.num_frames);
    }

    pub(crate) fn create_header(&self) -> Vec<u8> {
        const HEADER_SIZE: usize = std::mem::size_of::<VidFileHeader>();
        let mut buf = vec![0u8; HEADER_SIZE];
        self.fill_header(buf.as_mut_slice());
        buf
    }

    pub(crate) fn write_header<W>(&self, writer: &mut W) -> Result<usize, TranscodeError>
    where
        W: Write,
    {
        const HEADER_SIZE: usize = std::mem::size_of::<VidFileHeader>();
        let mut buf = [0u8; HEADER_SIZE];
        self.fill_header(&mut buf);
        match writer.write(&buf)? {
            sz if sz == HEADER_SIZE => Ok(HEADER_SIZE),
            _ => Err(TranscodeError::ProblemWritingToFile),
        }
    }
}
