#![feature(io_slice_advance)]
#![feature(async_closure)]
extern crate core;

use thiserror::Error;

pub const MAX_CPU_WORKERS: usize = 512;
pub const MAX_IO_WORKERS: usize = 1;

pub mod meta;
pub mod rules;
pub mod strategy;
pub mod transcoder;

pub mod controller;
pub mod data;
mod extra;
pub mod inspect;
mod spec;
mod util;

pub fn run_glommio() {
    transcoder::run_glommio()
}

// One erring to rule them all...
#[derive(Error, Debug)]
pub enum TranscodeError {
    #[error("File too short")]
    FileTooShort,
    #[error("Invalid Header")]
    InvalidHeader,
    #[error("Input Truncated")]
    InputTruncated,
    #[error("Output Truncated")]
    OutputTruncated,
    #[error("Invalid Rules")]
    InvalidRules,
    #[error("Unsupported too big Frame Dimensions {width} {height}")]
    UnsupportedFrameDimensionsTooBig { width: u32, height: u32 },
    #[error("Unsupported too small Frame Dimensions {width} {height}")]
    UnsupportedFrameDimensionsTooSmall { width: u32, height: u32 },
    #[error("Unexpected IO error")]
    IoErr(#[from] std::io::Error),
    #[error("Problem writing to file")]
    ProblemWritingToFile,
    #[error("Too Many Processing Rules")]
    TooManyRules,
    #[error("Unexpected Json error")]
    JsonErr(#[from] serde_json::Error),
    #[error("Too Many Threads")]
    TooManyThreads(usize),
    #[error("Invalid Alpha Channel Value")]
    InvalidAlpha,
    #[error("Generic threading error")]
    ThreadErr,
    #[error("Crossbeam error")]
    CrossBeamErr,
    #[error("Tokio error")]
    TokioErr,
    #[error("Queue Full")]
    QueueFull,
    #[error("Job ID Not Found")]
    JobNotFound,
    #[error("Missing Rules")]
    MissingRules,
    #[error("Unsupported")]
    Unsupported,
}
