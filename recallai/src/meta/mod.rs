use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::data::Dim;
use crate::rules::{PartialRules, Rules};
use crate::spec::VidFileHeader;
use crate::TranscodeError;

// below this DCT is not generally viable
const MIN_DIM: Dim = Dim {
    width: 16,
    height: 16,
};

// represents 2x current UHD or 768MB/frame
const MAX_DIM: Dim = Dim {
    width: 16384,
    height: 16384,
};

// MetaHeader is same as VidFileHeader, but we want to decouple the exact file layout
// from the derived metadata
// Note, we don't want to allow outsiders to manipulate our validated fields
// thus this is an opaque data-structure (crate-pub)
#[derive(Debug, Copy, Clone)]
pub struct MetaHeader {
    pub(crate) dim: Dim,
    pub(crate) num_frames: u32,
    pub(crate) frame_size: usize,
}

fn check_bounds(tgt: &Dim, bounds: (&Dim, &Dim)) -> Result<(), TranscodeError> {
    let (min, max) = bounds;
    if tgt.width < min.width || tgt.height < min.height {
        Err(TranscodeError::UnsupportedFrameDimensionsTooSmall {
            width: tgt.width,
            height: tgt.height,
        })
    } else if tgt.width > max.width || tgt.height > max.height {
        Err(TranscodeError::UnsupportedFrameDimensionsTooBig {
            width: tgt.width,
            height: tgt.height,
        })
    } else {
        Ok(())
    }
}

impl MetaHeader {
    pub(crate) fn from_vid_header(
        vid_file_header: VidFileHeader,
    ) -> Result<MetaHeader, TranscodeError> {
        check_bounds(&vid_file_header.dim, (&MIN_DIM, &MAX_DIM))?;

        let frame_size = vid_file_header.dim.calc_size();
        let frame_size = frame_size as usize;
        Ok(Self {
            dim: vid_file_header.dim,
            num_frames: vid_file_header.num_frames,
            frame_size,
        })
    }

    pub fn from_file_and_partial_rules<AP>(
        file_name: AP,
        partial_rules: PartialRules,
    ) -> Result<(MetaHeader, Rules), TranscodeError>
    where
        AP: AsRef<Path>,
    {
        let mh = Self::from_file(file_name)?;
        let rules = partial_rules.build(mh)?;
        Ok((mh, rules))
    }

    // Read and validate the metadata for a source file name
    pub fn from_file<AP>(file_name: AP) -> Result<MetaHeader, TranscodeError>
    where
        AP: AsRef<Path>,
    {
        // buffering is excessive here
        let mut file = File::open(file_name).map_err(TranscodeError::IoErr)?;
        Self::from_reader(&mut file)
    }

    pub fn from_reader_and_partial_rules<R>(
        reader: &mut R,
        partial_rules: PartialRules,
    ) -> Result<(MetaHeader, Rules), TranscodeError>
    where
        R: Read,
    {
        let mh = Self::from_reader(reader)?;
        let rules = partial_rules.build(mh)?;
        Ok((mh, rules))
    }

    // Read and validate the metadata for an already opened stream
    // Note, the stream position will be the first byte after the header
    pub fn from_reader<R>(mut reader: &mut R) -> Result<MetaHeader, TranscodeError>
    where
        R: Read,
    {
        let (file_header, _) = VidFileHeader::open_from_reader(&mut reader)?; // unbuffered
        let meta_header = MetaHeader::from_vid_header(file_header)?; // interpret / validate header
        Ok(meta_header)
    }
}
