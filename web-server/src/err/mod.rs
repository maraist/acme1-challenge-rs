use crate::response::Response1;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Default, Serialize)]
pub struct ResponseErrorItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Cow<'static, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Cow<'static, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Cow<'static, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Cow<'static, str>>,
}

impl ResponseErrorItem {
    pub fn title_cow(title: Cow<'static, str>) -> Self {
        ResponseErrorItem {
            title: Some(title),
            ..Default::default()
        }
    }
    pub fn title_str(title: &'static str) -> Self {
        ResponseErrorItem {
            title: Some(Cow::Borrowed(title)),
            ..Default::default()
        }
    }
    pub fn title(title: String) -> Self {
        ResponseErrorItem {
            title: Some(Cow::Owned(title)),
            ..Default::default()
        }
    }
    pub fn title_source(title: String, source: String) -> Self {
        ResponseErrorItem {
            title: Some(Cow::Owned(title)),
            source: Some(Cow::Owned(source)),
            ..Default::default()
        }
    }
    pub fn title_source_str(title: &'static str, source: &'static str) -> Self {
        ResponseErrorItem {
            title: Some(Cow::Borrowed(title)),
            source: Some(Cow::Borrowed(source)),
            ..Default::default()
        }
    }
}

#[derive(Error, Debug)]
pub enum RequestError1 {
    #[error("Validation {0:?} error")]
    Validation(Vec<ResponseErrorItem>),

    #[error("Missing Source Resource {0:?}")]
    MissingSourceResource(ResponseErrorItem),
    #[error("Job Not Found")]
    JobNotFound(ResponseErrorItem),
    #[error("Unexpected system error")]
    SystemError(Cow<'static, str>),
}
impl RequestError1 {
    pub fn sys_str(msg: &'static str) -> Self {
        Self::SystemError(Cow::Borrowed(msg))
    }
    pub fn sys(msg: String) -> Self {
        Self::SystemError(Cow::Owned(msg))
    }
}

impl IntoResponse for RequestError1 {
    fn into_response(self) -> axum::response::Response {
        use RequestError1::*;
        let tuple = match self {
            Validation(v) => (StatusCode::UNPROCESSABLE_ENTITY, Json(Response1::errs(v))),
            MissingSourceResource(v) => (StatusCode::UNPROCESSABLE_ENTITY, Json(Response1::err(v))),
            SystemError(v) => {
                let e = ResponseErrorItem::title_cow(v);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(Response1::err(e)))
            }
            JobNotFound(v) => (StatusCode::BAD_REQUEST, Json(Response1::err(v))),
        };
        tuple.into_response()
    }
}
