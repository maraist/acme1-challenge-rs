use axum::body::Body;
use axum::http::{header, Uri};
use axum::response::{IntoResponse, Response};

pub async fn sample_rules(_uri: Uri) -> impl IntoResponse {
    const CHARS: &'static [u8] = include_bytes!("../../static/sample-rules.json");
    // let bytes = Vec::from(chars);
    // ([(header::CONTENT_TYPE, mime::APPLICATION_JSON)], Body::from(chars))
    Response::builder()
        .header(
            header::CONTENT_TYPE.to_string(),
            mime::APPLICATION_JSON.to_string(),
        )
        .body(Body::from(CHARS))
        .unwrap()
}

pub async fn sample_rvid() -> &'static [u8] {
    const BYTES: &'static [u8] = include_bytes!("../../static/sample-input.rvid");
    BYTES
}
