use crate::data::OutHandle::HTTP_POST;
use crate::spec::BYTES_PER_PIXEL;
use serde::Deserialize;

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct Dim {
    pub width: u32,
    pub height: u32,
}

impl Dim {
    // Buffer bytes needed to encode this dimension
    pub const fn calc_size(&self) -> usize {
        self.calc_area(BYTES_PER_PIXEL) as usize
    }

    // Buffer bytes needed to encode this dimension
    pub const fn calc_area(&self, bytes_per_pixel: u32) -> u32 {
        self.width * self.height * bytes_per_pixel
    }

    // Buffer bytes to skip to get to the same column in the next row
    pub const fn calc_stride(&self, bytes_per_pixel: u32) -> u32 {
        self.width * bytes_per_pixel
    }
}

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct Pos {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct Viewport {
    // upper left
    pub ul: Pos,
    // height/width
    pub dim: Dim,
}

pub enum OutHandle {
    #[allow(non_camel_case_types)]
    HTTP_POST(String),
    #[allow(non_camel_case_types)]
    HTTP_PUT(String),
    Stdout,
    File(String),
    Null,
}
impl OutHandle {
    pub fn post(uri: String) -> Self {
        HTTP_POST(uri)
    }
    pub fn from_str(uri: String) -> Self {
        if uri == "-" {
            OutHandle::Stdout
        } else if uri.starts_with("http") {
            OutHandle::HTTP_PUT(uri)
        } else {
            OutHandle::File(uri)
        }
    }
}

pub enum InHandle {
    #[allow(non_camel_case_types)]
    HTTP_GET(String),
    File(String),
    Null,
    Stdin,
}
impl InHandle {
    pub fn from_str(uri: String) -> Self {
        if uri == "-" {
            InHandle::Stdin
        } else if uri.starts_with("http") {
            InHandle::HTTP_GET(uri)
        } else {
            InHandle::File(uri)
        }
    }
}
