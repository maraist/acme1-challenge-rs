use std::borrow::Cow;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use axum_macros::debug_handler;
use recallai_vid_transcode::controller;
use recallai_vid_transcode::controller::Status::QUEUED;
use recallai_vid_transcode::controller::{Job, LocationType};
use ulid::Ulid;

use crate::err::{RequestError1, ResponseErrorItem};
use crate::response::{JobInfo, ResponseJobInfo, ResponseJobList};
use crate::server::AppState;
use crate::spec::RecomposeJson;

pub async fn recompose(
    State(state): State<AppState>,
    Json(req_json): Json<RecomposeJson>,
) -> Result<Json<ResponseJobInfo>, RequestError1> {
    let vid_url = if let Some(vid_url) = req_json.video_url {
        if vid_url.starts_with("http") {
            LocationType::HTTP_GET(vid_url)
        } else {
            LocationType::File(vid_url)
        }
    } else {
        return Err(RequestError1::Validation(vec![ResponseErrorItem {
            status: None,
            detail: None,
            source: None, // TODO
            title: Some(Cow::Borrowed("missing video_url")),
        }]));
    };
    let dest_url = if let Some(dest_url) = req_json.dest_url {
        if dest_url.starts_with("http") {
            LocationType::HTTP_POST(dest_url)
        } else {
            LocationType::File(dest_url)
        }
    } else {
        return Err(RequestError1::Validation(vec![ResponseErrorItem {
            status: None,
            detail: None,
            source: None, // TODO
            title: Some(Cow::Borrowed("missing dest_url")),
        }]));
    };
    let rules_url = if let Some(rules_url) = req_json.rules_url {
        if rules_url.starts_with("http") {
            LocationType::HTTP_GET(rules_url)
        } else {
            LocationType::File(rules_url)
        }
    } else {
        return Err(RequestError1::Validation(vec![ResponseErrorItem {
            status: None,
            detail: None,
            source: None, // TODO
            title: Some(Cow::Borrowed("missing rules_url")),
        }]));
    };
    let mut ctx = state.controller.lock().await;
    let job = Job {
        id: Ulid::new(),
        video_url: vid_url,
        dest_url: dest_url,
        rule_url: rules_url,
        running: Arc::new(Default::default()),
        num_frames: None,
        cur_frame: Arc::new(Default::default()),
        status: QUEUED,
        took: Default::default(),
    };
    if let Ok(j) = ctx.create_job_async(job).await {
        let job_str = j.to_string();

        let job_info = JobInfo {
            id: job_str.clone(),
            status: String::from(QUEUED),
            cur_frame: None,
            max_frames: None,
            poll_url: Some(format!("{}/job/{job_str}", state.base_url)),
            cancel_url: Some(format!("{}/job/{job_str}/cancel", state.base_url)),
            took_sec: None,
        };
        Ok(Json(ResponseJobInfo::ok(job_info)))
    } else {
        Err(RequestError1::Validation(vec![ResponseErrorItem {
            status: None,
            detail: None,
            source: None, // TODO
            title: Some(Cow::Borrowed("missing rules_url")),
        }]))
    }
}

fn mk_job_info(base_url: &str, job: Job) -> JobInfo {
    let job_str = job.id.to_string();
    let max_frames = if let Some(v) = job.num_frames {
        Some(v)
    } else {
        None
    };

    let took = if job.took.as_millis() == 0 {
        None
    } else {
        Some(job.took.as_secs_f64())
    };
    JobInfo {
        id: job_str.clone(),
        status: String::from(job.status),
        cur_frame: Some(job.cur_frame.load(SeqCst)),
        max_frames,
        poll_url: Some(format!("{}/job/{job_str}", base_url)),
        cancel_url: Some(format!("{}/job/{job_str}/cancel", base_url)),
        took_sec: took,
    }
}

pub async fn job_list_all(
    State(state): State<AppState>,
) -> Result<Json<ResponseJobList>, RequestError1> {
    job_list_inner(true, State(state)).await
}
pub async fn job_list(
    State(state): State<AppState>,
) -> Result<Json<ResponseJobList>, RequestError1> {
    job_list_inner(false, State(state)).await
}
async fn job_list_inner(
    all: bool,
    State(state): State<AppState>,
) -> Result<Json<ResponseJobList>, RequestError1> {
    let ctrl = state.controller.lock().await;

    match ctrl.get_job_list(all) {
        Ok(jobs) => {
            let out: Vec<JobInfo> = jobs
                .into_iter()
                .map(|j| mk_job_info(&state.base_url, j))
                .collect();
            Ok(Json(ResponseJobList {
                data: Some(out),
                errors: None,
                status: Cow::Borrowed("Ok"),
            }))
        }
        Err(_e) => Err(RequestError1::SystemError(Cow::Borrowed(
            "Couldn't find job listing",
        ))),
    }
}

#[debug_handler]
pub async fn job_status(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ResponseJobInfo>, RequestError1> {
    let ctrl = state.controller.lock().await;
    let Ok(id) = Ulid::from_string(id.as_str()) else {
        return Err(RequestError1::MissingSourceResource(
            ResponseErrorItem::title(String::from("id")),
        ));
    };
    match ctrl.get_job_status(id) {
        Ok(job) => {
            let max_frames = if let Some(v) = job.num_frames {
                Some(v)
            } else {
                None
            };
            let job_str = job.id.to_string();
            let took = if job.took.as_millis() == 0 {
                None
            } else {
                Some(job.took.as_secs_f64())
            };
            Ok(Json(ResponseJobInfo::ok(JobInfo {
                id: job_str.clone(),
                status: String::from(job.status),
                cur_frame: Some(job.cur_frame.load(SeqCst)),
                max_frames,
                poll_url: Some(format!("{}/job/{job_str}", state.base_url)),
                cancel_url: Some(format!("{}/job/{job_str}/cancel", state.base_url)),
                took_sec: took,
            })))
        }
        Err(e) => Err(RequestError1::JobNotFound(ResponseErrorItem::title(
            e.to_string(),
        ))),
    }
}

pub async fn job_cancel(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ResponseJobInfo>, RequestError1> {
    let mut ctrl = state.controller.lock().await;
    let Ok(id) = Ulid::from_string(id.as_str()) else {
        return Err(RequestError1::MissingSourceResource(
            ResponseErrorItem::title(String::from("id")),
        ));
    };
    match ctrl.cancel_job(id) {
        Ok(_) => Ok(Json(ResponseJobInfo::ok(JobInfo {
            id: id.to_string(),
            status: String::from(controller::Status::DELETED),
            cur_frame: None,
            max_frames: None,
            poll_url: None,
            cancel_url: None,
            took_sec: None,
        }))),
        Err(e) => Err(RequestError1::JobNotFound(ResponseErrorItem::title(
            e.to_string(),
        ))),
    }
}
