use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "vid-server", about = "Video Rule Processor Web Server")]
struct Opt {
    // TCP port to listen on
    #[structopt(short, long, default_value = "3000")]
    port: u16,

    // if 0, use num-cpu threads (default: 4), else specify number of transcoder workers
    #[structopt(long = "num-threads", default_value = "4")]
    num_threads: u32,

    // The URL exposed in help messages
    #[structopt(long)]
    base_url: Option<String>,
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();
    recallai_ws::server::launch_server(opt.port, opt.num_threads as u16, opt.base_url).await;
}
