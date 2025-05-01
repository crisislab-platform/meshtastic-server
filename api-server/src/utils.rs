use axum::{http::StatusCode, response::IntoResponse, Json};
use log::error;
use serde::Serialize;

pub enum SimpleResponse {
    Ok,
    Err(StatusCode, String),
}

#[derive(Serialize)]
struct SimpleResponseError {
    error: String,
}

impl SimpleResponse {
    pub fn log(self) -> Self {
        if let SimpleResponse::Err(status_code, message) = &self {
            error!("{} (error reported with status {})", message, status_code);
        }

        return self;
    }
}

impl IntoResponse for SimpleResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            SimpleResponse::Ok => StatusCode::OK.into_response(),
            SimpleResponse::Err(status_code, message) => {
                (status_code, Json(SimpleResponseError { error: message })).into_response()
            }
        }
    }
}
