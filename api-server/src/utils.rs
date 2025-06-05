use axum::{http::StatusCode, response::IntoResponse, Json};
use log::error;
use serde::Serialize;

pub enum FallibleJsonResponse<T: Serialize> {
    Ok(T),
    Err(StatusCode, String),
}

#[derive(Serialize)]
struct SingletonError {
    error: String,
}

impl<T: Serialize> IntoResponse for FallibleJsonResponse<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            FallibleJsonResponse::Ok(data) => (StatusCode::OK, Json(data)).into_response(),
            FallibleJsonResponse::Err(status_code, message) => {
                (status_code, Json(SingletonError { error: message })).into_response()
            }
        }
    }
}

impl<T: Serialize> FallibleJsonResponse<T> {
    pub fn log(self) -> Self {
        if let FallibleJsonResponse::Err(status_code, message) = &self {
            error!("{} (error reported with status {})", message, status_code);
        }

        return self;
    }
}

pub enum StringOrEmptyResponse {
    Ok,
    Err(StatusCode, String),
}

impl IntoResponse for StringOrEmptyResponse {
    fn into_response(self) -> axum::response::Response {
        match self {
            StringOrEmptyResponse::Err(status_code, message) => (status_code, message).into_response(),
            StringOrEmptyResponse::Ok => StatusCode::OK.into_response(),
        }
    }
}

impl StringOrEmptyResponse {
    pub fn log(self) -> Self {
        if let StringOrEmptyResponse::Err(status_code, message) = &self {
            error!("{} (error reported with status {})", message, status_code);
        }

        self
    }
}
