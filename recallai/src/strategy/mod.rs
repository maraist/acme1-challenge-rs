use crate::{TranscodeError, MAX_CPU_WORKERS, MAX_IO_WORKERS};

#[derive(Debug, Clone)]
pub enum IoStrategy {
    SequentialSt,
    SequentialIoMt {
        num_cpu_workers: u16,
    },
    ParallelIoMt {
        num_cpu_workers: u16,
    },
    #[cfg(feature = "glommio")]
    Glommio {
        num_cpu_workers: u16,
    },
    Tokio {
        num_cpu_workers: u16,
    },
}

impl IoStrategy {
    pub fn new_io_mt(
        num_io_workers: usize,
        num_cpu_workers: usize,
    ) -> Result<Self, TranscodeError> {
        if num_cpu_workers > MAX_CPU_WORKERS {
            Err(TranscodeError::TooManyThreads(num_cpu_workers))
        } else if num_io_workers > MAX_IO_WORKERS {
            Err(TranscodeError::TooManyThreads(num_io_workers))
        } else {
            // TODO implement ParallelIoMt
            // for now, just asset 1 io worker and revert to sequential source IO
            Ok(IoStrategy::SequentialIoMt {
                num_cpu_workers: num_cpu_workers as u16,
            })
        }
    }
    #[cfg(feature = "glommio")]
    pub fn new_glommio(num_cpu_workers: usize) -> Result<Self, TranscodeError> {
        Ok(IoStrategy::Glommio {
            num_cpu_workers: num_cpu_workers as u16,
        })
    }

    pub fn new_tokio(num_cpu_workers: usize) -> Result<Self, TranscodeError> {
        Ok(IoStrategy::Tokio {
            num_cpu_workers: num_cpu_workers as u16,
        })
    }
    pub fn new_mp_mc(num_cpu_workers: usize) -> Result<Self, TranscodeError> {
        if num_cpu_workers > MAX_CPU_WORKERS {
            Err(TranscodeError::TooManyThreads(num_cpu_workers))
        } else {
            Ok(IoStrategy::ParallelIoMt {
                num_cpu_workers: num_cpu_workers as u16,
            })
        }
    }
    pub fn new_seq_mt(num_cpu_workers: usize) -> Result<Self, TranscodeError> {
        if num_cpu_workers > MAX_CPU_WORKERS {
            Err(TranscodeError::TooManyThreads(num_cpu_workers))
        } else {
            Ok(IoStrategy::SequentialIoMt {
                num_cpu_workers: num_cpu_workers as u16,
            })
        }
    }
}
