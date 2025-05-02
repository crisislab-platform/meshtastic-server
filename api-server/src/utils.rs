use axum::{http::StatusCode, response::IntoResponse, Json};
use log::error;
use serde::Serialize;

pub enum JsonAndStatusResponse<T: Serialize> {
    Ok(T),
    Err(StatusCode, String),
}

#[derive(Serialize)]
struct SingletonError {
    error: String,
}

impl<T: Serialize> IntoResponse for JsonAndStatusResponse<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            JsonAndStatusResponse::Ok(data) => (StatusCode::OK, Json(data)).into_response(),
            JsonAndStatusResponse::Err(status_code, message) => {
                (status_code, Json(SingletonError { error: message })).into_response()
            }
        }
    }
}

impl<T: Serialize> JsonAndStatusResponse<T> {
    pub fn log(self) -> Self {
        if let JsonAndStatusResponse::Err(status_code, message) = &self {
            error!("{} (error reported with status {})", message, status_code);
        }

        return self;
    }
}
