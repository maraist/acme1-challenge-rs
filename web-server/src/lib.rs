pub mod err;
mod handlers;
pub mod response;
pub mod spec;

pub mod server;

// This is the redirect URL used if not provided by CLI
const BASE_URL: &'static str = "http://localhost:80";
