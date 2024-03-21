use crate::handlers::crc::handle_crc;
use crate::handlers::help::{css, help, index};
use crate::handlers::recompose::{job_cancel, job_list, job_list_all, job_status, recompose};
use crate::handlers::sample_rvid::{sample_rules, sample_rvid};
use crate::BASE_URL;
use axum::routing::{get, post};
use axum::Router;
use recallai_vid_transcode::controller::ControllerStatus;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub(crate) struct AppState {
    pub controller: Arc<tokio::sync::Mutex<ControllerStatus>>,
    pub base_url: String,
}
pub async fn launch_server(port: u16, num_cpu_workers: u16, base_url: Option<String>) {
    let base_url = base_url.unwrap_or(String::from(BASE_URL));
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_stream_to_file=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let controller =
        recallai_vid_transcode::controller::ControllerStatus::new(num_cpu_workers).await;
    let state = AppState {
        controller,
        base_url,
    };
    let app = Router::new()
        .route("/recompose", post(recompose))
        .route("/job/list", get(job_list))
        .route("/job/list/all", get(job_list_all))
        .route("/job/:id", get(job_status).delete(job_cancel))
        .route("/job/:id/cancel", get(job_cancel).delete(job_cancel))
        .route("/help", get(help))
        .route("/crc", post(handle_crc))
        .route("/", get(help))
        .route("/index.htm", get(help))
        .route("/index.html", get(help))
        .route("/static/index.html", get(index))
        .route("/static/tailslim.css", get(css))
        .route("/static/sample.rvid", get(sample_rvid))
        .route("/static/sample-rules.json", get(sample_rules))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}
