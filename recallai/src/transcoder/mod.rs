use crate::data::{InHandle, OutHandle};
use ulid::Ulid;

use crate::rules::PartialRules;
use crate::strategy::IoStrategy;
use crate::TranscodeError;

mod mpmtmc;
mod spmtsc;
mod spstsc;

mod bufstack;
#[cfg(feature = "glommio")]
mod glommio;
pub(crate) mod tokioworker;

// convenience function for glommio debug-run
pub fn run_glommio() {
    #[cfg(feature = "glommio")]
    glommio::run_glommio()
}

// convenience function for tokio debug
pub async fn run_tokio(num_cpu_workers: u16) {
    // tokioworker::test_flow_null_in_out_x(4).await;
    tokioworker::test_flow_xx(num_cpu_workers).await;
}

// A MegaMorphic Transcoder Object (e.g. polymorphic-lite)
//
// This is a proxy that approximates polymorphism (more correctly mega-morphism) such that
// the caller can separately define different segments of the pipeline life cycle.
// It is assumed that this pipeline path is called significantly less often than any other
// stage, thus the overhead of the enum-dispatch is minimal.
// In most cases, there will only be a single dispatch.
//
// Note, Box<dyn MyTrait> does not apply here because we make heavy use of template inlining.
// most notably stdin v.s. file v.s. in-memory-buffer Read/Write seekable structures
pub enum TranscoderProxy {
    SpStSc(spstsc::SpStScTranscoder),
    SpMtSc(spmtsc::SpMtScTranscoder),
    MpMtMc(mpmtmc::MpMtMcTranscoder),
    #[cfg(feature = "glommio")]
    Glommio(glommio::GlommioTranscoder),
    Tokio(tokioworker::TokioTranscoder),
}

impl TranscoderProxy {
    pub fn transcode_handles(
        &mut self,
        file_name_in: InHandle,
        file_name_out: OutHandle,
    ) -> Result<(), TranscodeError> {
        use TranscoderProxy::*;
        match self {
            SpStSc(t) => t.transcode_handles(file_name_in, file_name_out),
            SpMtSc(t) => t.transcode_handles(file_name_in, file_name_out),
            MpMtMc(t) => t.transcode_handles(file_name_in, file_name_out),
            #[cfg(feature = "glommio")]
            Glommio(t) => t.transcode_handles(file_name_in, file_name_out),
            Tokio(t) => t
                .transcode_handles_blocking(file_name_in, file_name_out)
                .map(|_v| ()),
        }
    }
}

// Construct an execution pipeline given the rules and strategy and validated video-header
pub fn load_transcoder(
    strategy: IoStrategy,
    rules: PartialRules,
) -> Result<TranscoderProxy, TranscodeError> {
    use IoStrategy::*;
    let res = match strategy {
        SequentialSt => TranscoderProxy::SpStSc(spstsc::SpStScTranscoder { rules: Some(rules) }),
        SequentialIoMt { num_cpu_workers } => TranscoderProxy::SpMtSc(spmtsc::SpMtScTranscoder {
            rules: Some(rules),
            num_cpu_workers,
        }),
        ParallelIoMt { num_cpu_workers } => TranscoderProxy::MpMtMc(mpmtmc::MpMtMcTranscoder {
            rules: Some(rules),
            num_cpu_workers,
        }),
        #[cfg(feature = "glommio")]
        Glommio { num_cpu_workers } => TranscoderProxy::Glommio(glommio::GlommioTranscoder {
            num_workers: num_cpu_workers,
            rules: Some(rules),
        }),
        Tokio { num_cpu_workers } => TranscoderProxy::Tokio(tokioworker::TokioTranscoder {
            num_workers: num_cpu_workers,
            jobid: Ulid::new(),
            rules: Some(rules),
            debug: false,
            collect_stats: false,
        }),
    };
    Ok(res)
}
