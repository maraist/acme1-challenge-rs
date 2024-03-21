use std::ops::Range;
use std::path::PathBuf;

use recallai_vid_transcode::inspect::extract_images;
use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "vidrules", about = "Video Rule Processor")]
struct Opt {
    // Source file-name; default to stdin
    #[structopt(short = "i", long)]
    src: PathBuf,

    // Source file-name; default to stdin
    #[structopt(short = "o", long)]
    out_dir: PathBuf,

    // Start writing images at frame_min
    #[structopt(short = "q", long, default_value = "70")]
    jpeg_quality: u8,

    // Start writing images at frame_min
    #[structopt(long)]
    frame_min: u32,

    // Start writing images at frame_max
    #[structopt(long)]
    frame_max: Option<u32>,

    // Scale down to a max specified width
    #[structopt(long = "max-width")]
    max_width: Option<u32>,
}

// This program allows me to visualize the imagery
// It is not optimized, nor threaded; I used bash with external parallel executors to
// subdivide the image frames
fn main() {
    let opt = Opt::from_args();
    println!("opt: {opt:?}");

    let range = if let Some(frame_max) = opt.frame_max {
        std::ops::Range {
            start: opt.frame_min as usize,
            end: frame_max as usize,
        }
    } else {
        Range {
            start: opt.frame_min as usize,
            end: usize::MAX,
        }
    };
    let tgt_dir = String::from(opt.out_dir.to_str().expect("missin out dir"));
    extract_images(
        opt.src,
        tgt_dir,
        opt.jpeg_quality,
        range,
        opt.max_width.unwrap_or(4096),
    );
}
