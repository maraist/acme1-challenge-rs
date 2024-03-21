use crate::err::ResponseErrorItem;
use serde::Serialize;
use std::borrow::Cow;

#[derive(Debug, Default, Serialize)]
pub struct JobInfo {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur_frame: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took_sec: Option<f64>,
}
#[derive(Debug, Default, Serialize)]
pub struct ResponseJobInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ResponseErrorItem>>,
    pub status: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JobInfo>,
}
#[derive(Debug, Default, Serialize)]
pub struct ResponseJobList {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ResponseErrorItem>>,
    pub status: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<JobInfo>>,
}

impl ResponseJobInfo {
    pub fn ok(job_info: JobInfo) -> Self {
        Self {
            status: Cow::Borrowed("Ok"),
            errors: None,
            data: Some(job_info),
        }
    }
    pub fn err(reason: ResponseErrorItem) -> Self {
        Self {
            status: Cow::Borrowed("Error"),
            errors: Some(vec![reason]),
            data: None,
        }
    }
    pub fn errs(reason: Vec<ResponseErrorItem>) -> Self {
        Self {
            status: Cow::Borrowed("Error"),
            errors: Some(reason),
            data: None,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Response1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ResponseErrorItem>>,
    pub status: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<String>>,
}

impl Response1 {
    pub fn ok() -> Self {
        Self {
            status: Cow::Borrowed("Ok"),
            errors: None,
            data: None,
        }
    }
    pub fn err(reason: ResponseErrorItem) -> Self {
        Self {
            status: Cow::Borrowed("Error"),
            errors: Some(vec![reason]),
            data: None,
        }
    }
    pub fn errs(reason: Vec<ResponseErrorItem>) -> Self {
        Self {
            status: Cow::Borrowed("Error"),
            errors: Some(reason),
            data: None,
        }
    }
}
