use crate::data::Dim;
use crate::meta::MetaHeader;
use crate::spec::VidFileHeader;
use image::codecs::jpeg::JpegEncoder;
use image::flat::SampleLayout;
use image::imageops::thumbnail;
use image::{Rgb, RgbImage};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::Path;
use std::slice;

pub fn extract_images<AP1>(
    in_fn: AP1,
    out_path: String,
    quality: u8,
    frames: Range<usize>,
    max_width: u32,
) where
    AP1: AsRef<Path>,
{
    let mut file = File::open(in_fn).expect("File not found ");
    let md = MetaHeader::from_reader(&mut file).expect("Could not read header");
    let to_skip = VidFileHeader::get_header_size() + frames.start * md.frame_size;
    file.seek(SeekFrom::Start(to_skip as u64))
        .expect("Could not seek to pos");
    match std::fs::metadata(&out_path) {
        Ok(md) => {
            if !md.is_dir() {
                panic!("out is not a directory")
            }
            // else good
        }
        Err(_) => {
            std::fs::create_dir_all(&out_path).expect("couldn't create out dir");
        }
    }
    let read_size = md.frame_size;
    let mut buf = vec![0u8; read_size].into_boxed_slice();

    let mut out_file_path = out_path.clone();
    out_file_path.push_str("/"); // WINDOWS NOT SUPPORTED
    let reset_len = out_file_path.len();
    let layout = SampleLayout {
        channels: 3,
        channel_stride: 1,
        width: md.dim.width,
        width_stride: 3,
        height: md.dim.height,
        height_stride: (md.dim.height * 3) as usize, // hack
    };

    let (t_dim, rescale) = if false && max_width < md.dim.width {
        // something is wrong with the rescale.. disabling
        let t_width = md.dim.width.min(max_width);
        let t_height = ((t_width as f32) * (md.dim.height as f32 / md.dim.width as f32)) as u32;
        (
            Dim {
                width: t_width,
                height: t_height,
            },
            true,
        )
    } else {
        (md.dim, false)
    };
    let Dim {
        width: t_width,
        height: t_height,
    } = t_dim;
    for fr in frames {
        if let Ok(()) = file.read_exact(&mut buf) {
            out_file_path.truncate(reset_len);
            out_file_path.push_str(&format!("{fr:04}"));
            out_file_path.push_str(".jpg");

            if !rescale {
                let mut out = RgbImage::new(t_width, t_height);

                let mut lbuf = buf.as_mut();
                for row in 0..md.dim.height {
                    for col in 0..md.dim.width {
                        let pixel = Rgb([lbuf[1], lbuf[2], lbuf[0]]);
                        out.put_pixel(col, row, pixel);
                        lbuf = &mut lbuf[3..];
                    }
                }
                let mut out_file = File::create(&out_file_path).expect("couldn't create jpg file");
                let encoder = JpegEncoder::new_with_quality(&mut out_file, quality);
                out.write_with_encoder(encoder).expect("couldn't encode");
            } else {
                let mut lbuf = buf.as_mut();
                for _row in 0..md.dim.height {
                    for _col in 0..md.dim.width {
                        let [b, r, g] = [lbuf[0], lbuf[1], lbuf[2]];
                        lbuf[0] = r;
                        lbuf[1] = g;
                        lbuf[2] = b;
                        lbuf = &mut lbuf[3..];
                    }
                }
                let samples = unsafe { slice::from_raw_parts(buf.as_ptr(), buf.len()) };
                let buffer = image::flat::FlatSamples {
                    samples,
                    layout,
                    color_hint: None,
                };

                let view = match buffer.as_view::<Rgb<u8>>() {
                    Ok(view) => view,
                    Err(_) => panic!("couldn't map raw image buffer"),
                };

                thumbnail(&view, t_width, t_height)
                    .save(&out_file_path)
                    .expect("unable to generate thumbnail");
            }
        } else {
            return;
        }
    }
}
