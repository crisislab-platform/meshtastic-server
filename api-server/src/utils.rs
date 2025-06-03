use axum::{http::StatusCode, response::IntoResponse, Json};
use log::error;
use serde::Serialize;

pub enum JsonOrErrorResponse<T: Serialize> {
    Ok(T),
    Err(StatusCode, String),
}

#[derive(Serialize)]
struct SingletonError {
    error: String,
}

impl<T: Serialize> IntoResponse for JsonOrErrorResponse<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            JsonOrErrorResponse::Ok(data) => (StatusCode::OK, Json(data)).into_response(),
            JsonOrErrorResponse::Err(status_code, message) => {
                (status_code, Json(SingletonError { error: message })).into_response()
            }
        }
    }
}

impl<T: Serialize> JsonOrErrorResponse<T> {
    pub fn log(self) -> Self {
        if let JsonOrErrorResponse::Err(status_code, message) = &self {
            error!("{} (error reported with status {})", message, status_code);
        }

        return self;
    }
}
