use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::controller::dispatcher::tokio::TokioDispatcher;
use ulid::Ulid;

use crate::strategy::IoStrategy;
use crate::TranscodeError;

mod dispatcher;

#[derive(Debug, Default, Clone)]
pub enum LocationType {
    #[default]
    None,
    #[allow(non_camel_case_types)]
    HTTP_GET(String),
    #[allow(non_camel_case_types)]
    HTTP_PUT(String),
    #[allow(non_camel_case_types)]
    HTTP_POST(String),
    File(String),
}

#[derive(Default, Debug, Clone)]
pub enum Status {
    #[default]
    QUEUED,
    RUNNING,
    COMPLETED,
    DELETED,
}
impl From<Status> for String {
    fn from(value: Status) -> Self {
        match value {
            Status::QUEUED => String::from("QUEUED"),
            Status::RUNNING => String::from("RUNNING"),
            Status::COMPLETED => String::from("COMPLETED"),
            Status::DELETED => String::from("DELETED"),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Job {
    // ULID is a time-ordered varient of UUID with alternate string mapping for humans
    pub id: Ulid,
    pub video_url: LocationType,
    pub dest_url: LocationType,
    pub rule_url: LocationType,
    // can be used to shut off a running task gracefully
    pub running: Arc<AtomicBool>,
    // not known until processing time
    pub num_frames: Option<u32>,
    // allows direct updating
    pub cur_frame: Arc<AtomicU32>,
    pub status: Status,
    pub took: Duration,
}

pub struct ControllerStatus {
    max_queued_jobs: u32,
    #[allow(dead_code)]
    max_completed_jobs: u32,
    queued_jobs: VecDeque<Job>,
    running_job: Option<Job>,
    completed_jobs: HashMap<Ulid, Job>,
    #[allow(dead_code)]
    io_strategy: IoStrategy,
    dispatcher: TokioDispatcher,
}

impl ControllerStatus {
    pub async fn new(num_cpu_workers: u16) -> Arc<tokio::sync::Mutex<Self>> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(Ulid, Duration, u32, u32)>(32);
        let dispatcher = TokioDispatcher::start(num_cpu_workers, tx).await;
        let r = ControllerStatus {
            max_queued_jobs: 100,
            max_completed_jobs: 100,
            queued_jobs: VecDeque::new(),
            running_job: None,
            completed_jobs: HashMap::new(),
            io_strategy: IoStrategy::Tokio { num_cpu_workers },
            dispatcher,
        };

        let ret = Arc::new(tokio::sync::Mutex::new(r));
        let ctx = ret.clone();
        tokio::task::spawn(async move {
            while let Some((jid, dur, num_frames, max_frames)) = rx.recv().await {
                let mut ctx_ = ctx.lock().await;
                let _ = ctx_
                    .complete_job_async(jid, dur, num_frames, max_frames)
                    .await;
            }
        });
        ret
    }
    async fn send_job_async(&self, job: Job) -> Result<(), TranscodeError> {
        self.dispatcher.dispatch(job).await
    }

    async fn try_dispatch_async(&mut self) -> Result<(), TranscodeError> {
        if self.running_job.is_none() {
            if let Some(mut next_job) = self.queued_jobs.pop_front() {
                println!("dispatching job {}", next_job.id.to_string());
                next_job.status = Status::RUNNING;
                let job_ = next_job.clone();
                self.running_job = Some(next_job);
                return self.send_job_async(job_).await;
            }
        }
        Ok(())
    }

    pub async fn create_job_async(&mut self, mut job: Job) -> Result<Ulid, TranscodeError> {
        job.id = Ulid::from_datetime(SystemTime::now());
        if self.queued_jobs.len() >= self.max_queued_jobs as usize {
            println!("Queue depth exceeded");
            return Err(TranscodeError::QueueFull);
        }
        job.running.store(true, SeqCst);
        job.status = Status::QUEUED;
        let return_id = job.id.clone();
        self.queued_jobs.push_back(job);
        match self.try_dispatch_async().await {
            Ok(_) => Ok(return_id),
            Err(e) => Err(e),
        }
    }

    async fn complete_job_async(
        &mut self,
        id: Ulid,
        took: Duration,
        num_frames: u32,
        max_frames: u32,
    ) -> Result<(), TranscodeError> {
        println!(
            "job {} flagged as completed took: {}ms",
            id.to_string(),
            took.as_millis()
        );
        if let Some(mut job) = self.running_job.take() {
            if job.id == id {
                job.status = Status::COMPLETED;
                job.took = took;
                job.cur_frame.store(num_frames, SeqCst);
                job.num_frames = Some(max_frames);
                self.completed_jobs.insert(id, job);
            } else {
                println!("Warning, completing different job id {id} v.s. {}", job.id);
            }
        }
        self.try_dispatch_async().await
    }
    pub fn get_job_list(&self, include_finished: bool) -> Result<Vec<Job>, TranscodeError> {
        let mut ret: Vec<Job> = self.queued_jobs.iter().cloned().collect();
        if let Some(ref job) = self.running_job {
            ret.insert(0, job.clone());
        }
        if include_finished {
            if !self.completed_jobs.is_empty() {
                self.completed_jobs.values().for_each(|j| {
                    ret.push(j.clone());
                });
            }
        }
        Ok(ret)
    }

    pub fn get_job_status(&self, job_id: Ulid) -> Result<Job, TranscodeError> {
        if let Some(ref job) = self.running_job {
            if job.id == job_id {
                return Ok(job.clone());
            }
        }
        if let Some(job) = self.completed_jobs.get(&job_id) {
            return Ok(job.clone());
        }
        self.queued_jobs
            .iter()
            .find(|item| item.id == job_id)
            .cloned()
            .ok_or(TranscodeError::JobNotFound)
    }

    pub fn cancel_job(&mut self, job_id: Ulid) -> Result<(), TranscodeError> {
        if let Some(ref job) = self.running_job {
            println!("cancelling active job {}", job_id.to_string());
            job.running.store(false, SeqCst);
            Ok(())
        } else {
            let Some(idx) = self
                .queued_jobs
                .iter()
                .enumerate()
                .find(|(_idx, item)| item.id == job_id)
                .map(|(idx, _item)| idx)
            else {
                return Err(TranscodeError::JobNotFound);
            };

            println!("cancelling queued job {}", job_id.to_string());
            self.queued_jobs.remove(idx);
            Ok(())
        }
    }
}
