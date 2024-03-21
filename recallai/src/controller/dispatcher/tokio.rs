use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ulid::Ulid;

use crate::controller::{Job, LocationType};
use crate::data::{InHandle, OutHandle};
use crate::rules::{PartialRules, RuleDataJson, RuleOpts};
use crate::transcoder::tokioworker::TokioTranscoder;
use crate::TranscodeError;

pub(crate) struct TokioDispatcher {
    #[allow(dead_code)]
    shutdown: Arc<AtomicBool>,
    to_service: tokio::sync::mpsc::Sender<Option<Job>>,
}

impl TokioDispatcher {
    #[allow(dead_code)]
    pub(crate) async fn shutdown(&self) {
        if self.shutdown.load(SeqCst) {
            return;
        }
        self.shutdown.store(true, SeqCst);
        let _ = self.to_service.send(None).await; // wake up
                                                  // let _ = self.join_handle.await; // TODO err logging
    }

    pub(crate) async fn dispatch(&self, job: Job) -> Result<(), TranscodeError> {
        match self.to_service.send(Some(job)).await {
            Ok(_) => Ok(()),
            Err(_) => Err(TranscodeError::CrossBeamErr),
        }
    }

    async fn get_rules_from_job(job: &Job) -> Result<PartialRules, TranscodeError> {
        let rules = match job.rule_url {
            LocationType::File(ref name) => {
                let rules = RuleOpts::default()
                    .with_json_file_async(name.as_str())
                    .await?;
                rules
            }
            LocationType::HTTP_GET(ref url) => {
                let json = reqwest::get(url)
                    .await
                    .map_err(|_| TranscodeError::FileTooShort)?
                    .json::<RuleDataJson>()
                    .await
                    .map_err(|_| TranscodeError::FileTooShort)?;
                RuleOpts::default().with_json(json)?
            }
            _ => {
                return Err(TranscodeError::InvalidRules);
            }
        };
        Ok(rules)
    }

    fn convert_in_handle(lt: LocationType) -> Result<InHandle, TranscodeError> {
        let in_h = match lt {
            LocationType::HTTP_GET(u) => InHandle::HTTP_GET(u),
            LocationType::File(u) => InHandle::File(u),
            _ => {
                return Err(TranscodeError::Unsupported);
            }
        };
        Ok(in_h)
    }
    fn convert_out_handle(lt: LocationType) -> Result<OutHandle, TranscodeError> {
        let out_h = match lt {
            LocationType::HTTP_POST(u) => OutHandle::HTTP_POST(u),
            LocationType::HTTP_PUT(u) => OutHandle::HTTP_PUT(u),
            LocationType::File(u) => OutHandle::File(u),
            _ => {
                return Err(TranscodeError::Unsupported);
            }
        };
        Ok(out_h)
    }

    async fn process_job(
        debug: bool,
        num_workers: u16,
        job: Job,
    ) -> Result<(u32, u32), TranscodeError> {
        let rules = Self::get_rules_from_job(&job).await?;
        let mut xcoder = TokioTranscoder {
            num_workers,
            rules: Some(rules),
            debug,
            jobid: job.id.clone(),
            collect_stats: true,
        };

        let in_h = Self::convert_in_handle(job.video_url)?;
        let out_h = Self::convert_out_handle(job.dest_url)?;

        xcoder.transcode_handles(in_h, out_h).await
    }
    pub(crate) async fn start(
        num_cpu_workers: u16,
        on_job_finished: tokio::sync::mpsc::Sender<(Ulid, Duration, u32, u32)>,
    ) -> Self {
        let shutdown_ = Arc::new(AtomicBool::new(false));
        let shutdown = shutdown_.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<Job>>(24);
        let _join_handle = // TODO
            tokio::task::spawn(async move {
                println!("tokio job receiver dispatched");
                while let Some(msg) = rx.recv().await {
                    println!("tokio job receiver got job");
                    if shutdown.load(SeqCst) == true {
                        println!("tokio job receiver shutting down");
                        return;
                    }
                    if let Some(job) = msg {
                        if !job.running.load(SeqCst) {
                            println!("tokio job receiver next-job was already cancelled; skipping");
                            continue;
                        }
                        let job_id=job.id.clone();
                        println!("tokio job receiver next-job ; starting {}", job_id.to_string());
                        let st = Instant::now();
                        let (num_frames,max_frames) =
                        match TokioDispatcher::process_job(false, num_cpu_workers, job).await {
                            Ok((num_frames,max_frames)) => {
                               println!("success");
                                (num_frames,max_frames)
                            }
                            Err(e) => {
                                println!("job err: {e:?}");
                                (0,0)
                            }
                        };
                        let took = Instant::now().duration_since(st);
                        println!("tokio job receiver next-job ; finished {}", job_id.to_string());
                        let _ = on_job_finished.send((job_id, took, num_frames,max_frames)).await; // TODO
                    } else {
                        // ignore; might be a shutdown ping
                    }
                }
            });
        Self {
            shutdown: shutdown_,
            to_service: tx,
        }
    }
}
