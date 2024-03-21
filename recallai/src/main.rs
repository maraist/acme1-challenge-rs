use std::time::Instant;

use recallai_vid_transcode::data::{InHandle, OutHandle};
use structopt::StructOpt;

use recallai_vid_transcode::rules::RuleOpts;
use recallai_vid_transcode::strategy::IoStrategy;
use recallai_vid_transcode::strategy::IoStrategy::SequentialSt;
use recallai_vid_transcode::transcoder::load_transcoder;
use recallai_vid_transcode::TranscodeError;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "vidrules", about = "Video Rule Processor")]
struct Opt {
    // Source file-name; default to stdin
    #[structopt(short = "i", long)]
    src: Option<String>,

    // Rule file-name
    #[structopt(short = "r", long)]
    rules: String,

    // Target file-name ; default to stdout
    #[structopt(short = "o", long)]
    tgt: Option<String>,

    // Disable the default parallel execution
    #[structopt(short, long)]
    disable_parallel: bool,

    // Multiple Producer, Multiple Consumer - caps out at about 4 threads
    #[structopt(long = "mp-mc")]
    mp_mc: bool,

    // Uses Direct IO; Glommio (io_uring) with a single thread
    #[cfg(feature = "glommio")]
    #[structopt(long)]
    glommio: bool,

    // Uses Async IO ; Tokio
    #[structopt(long)]
    tokio: bool,

    // Display diagnostic info
    #[structopt(short, long)]
    debug: bool,

    // Display Timing info
    #[structopt(long)]
    timing: bool,

    // if 0, use num-cpu threads (default: 4), else specify number of transcoder workers
    #[structopt(long = "num-threads", default_value = "4")]
    num_threads: u32,

    // If enabled, will attempt to scan the rules for opaque outer-most Z-index layers
    // and delete any fully occluded lower Z-index layers.
    // By default, this is disabled
    #[structopt(long)]
    occlude_opaque_layers: bool,
}

fn main() -> Result<(), TranscodeError> {
    let mut opt = Opt::from_args();
    if opt.debug {
        println!("Opts: {:?}", opt);
    }

    // Our parallel executor relies upon multiple open files
    // to avoid L2 cache transfers
    reset_parallelism(&mut opt);

    let mut transcoder = {
        let st = Instant::now();

        // Parse / Decode Json rules
        let rule_opts = RuleOpts {
            occlude_opaque_layers: opt.occlude_opaque_layers,
            debug: opt.debug,
        };
        let rule_handle = get_video_in_handle(&Some(opt.rules.clone()))?;
        let rules = rule_opts.with_json_in_handle(rule_handle)?;

        // Choose a transcoder based on a strategy
        let transcoder_strategy = get_transcoder_strategy(&opt)?;

        let transcoder = load_transcoder(transcoder_strategy, rules)?;

        log_timing(&opt, st, "Decode Header and Rules");
        transcoder
    };
    let in_handle = get_video_in_handle(&opt.src)?;
    let out_handle = get_video_out_handle(&opt.tgt)?;

    // Do the main thing...
    let st = Instant::now();
    transcoder.transcode_handles(in_handle, out_handle)?;
    log_timing(&opt, st, "Transcoding");

    Ok(())
}

// Log based on whether we are recording timing the time since the last
// starting point
fn log_timing(opt: &Opt, st: Instant, msg: &str) {
    if opt.timing {
        let took = Instant::now().duration_since(st);
        println!("{msg} took: {}ms", took.as_millis());
    }
}

fn get_video_in_handle<A>(name: &Option<A>) -> Result<InHandle, TranscodeError>
where
    A: AsRef<str>,
{
    let r = match name {
        None => InHandle::Null,
        Some(str_) => {
            let str = str_.as_ref();
            if str == "-" {
                InHandle::Stdin
            } else if str.starts_with("http") {
                InHandle::HTTP_GET(str.to_string())
            } else {
                InHandle::File(str.to_string())
            }
        }
    };
    Ok(r)
}
fn get_video_out_handle(name: &Option<String>) -> Result<OutHandle, TranscodeError> {
    let r = match name {
        None => OutHandle::Null,
        Some(ref str) => {
            if str == "-" {
                OutHandle::Stdout
            } else if str.starts_with("http") {
                OutHandle::HTTP_POST(str.clone())
            } else {
                OutHandle::File(str.clone())
            }
        }
    };
    Ok(r)
}
fn reset_parallelism(opt: &mut Opt) {
    if !opt.disable_parallel && opt.mp_mc {
        if opt.src.is_none() || opt.tgt.is_none() {
            println!("Sorry, parallel stdin/out not currently supported");
            opt.disable_parallel = true;
        }
    }
    if !opt.disable_parallel && !opt.mp_mc {
        if opt.src.is_none() {
            opt.src = Some(String::from("-"));
        }
        if opt.tgt.is_none() {
            opt.tgt = Some(String::from("-"));
        }
    }
}

fn get_transcoder_strategy(opt: &Opt) -> Result<IoStrategy, TranscodeError> {
    let transcoder_strategy = if !opt.disable_parallel {
        let num_cpu_workers = if opt.num_threads == 0 {
            num_cpus::get()
        } else {
            opt.num_threads as usize
        };

        #[cfg(feature = "glommio")]
        if opt.glommio {
            return Ok(IoStrategy::new_glommio(num_cpu_workers)?);
        }
        if opt.tokio {
            return Ok(IoStrategy::new_tokio(num_cpu_workers)?);
        }

        if opt.mp_mc {
            // multi-producer, multi-consider (burns more CPU than single-producer/seq)
            // guard max-worker-count
            IoStrategy::new_mp_mc(num_cpu_workers)?
        } else {
            IoStrategy::new_seq_mt(num_cpu_workers)?
        }
    } else {
        SequentialSt
    };
    Ok(transcoder_strategy)
}
