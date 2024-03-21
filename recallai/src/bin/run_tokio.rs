use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "vidrules", about = "Video Rule Processor")]
struct Opt {
    // if 0, use num-cpu threads (default: 4), else specify number of transcoder workers
    #[structopt(long = "num-threads", default_value = "4")]
    num_threads: u32,
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    let num_cpu_workers = if opt.num_threads == 0 {
        num_cpus::get()
    } else {
        opt.num_threads as usize
    };
    recallai_vid_transcode::transcoder::run_tokio(num_cpu_workers as u16).await;
}
