use axum::http::header;
use axum::response::{Html, IntoResponse, Redirect, Response};

pub async fn help() -> impl IntoResponse {
    Redirect::to("/static/index.html")
}
pub async fn index() -> impl IntoResponse {
    let s = include_str!("../../static/index.html");
    Html(s)
}
pub async fn css() -> impl IntoResponse {
    let s = String::from(include_str!("../../static/tailslim.css"));
    // let headers = ([(header::CONTENT_TYPE, mime::TEXT_CSS_UTF_8 ),]);
    Response::builder()
        .header(
            header::CONTENT_TYPE.to_string(),
            mime::TEXT_CSS_UTF_8.to_string(),
        )
        .body(s)
        .unwrap()
}
