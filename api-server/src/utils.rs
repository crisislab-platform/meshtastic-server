use std::time::Duration;

use axum::{http::StatusCode, response::IntoResponse, Json};
use log::error;
use prost::Message;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;

use crate::proto::meshtastic::CrisislabMessage;

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
            StringOrEmptyResponse::Err(status_code, message) => {
                (status_code, message).into_response()
            }
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

/// Until the specified timeout has passed, this function will listen for messages from the mesh
/// via the given receiver and call the given callback on each decoded message.
///
/// If the callback would like to ignore the message it's given it should return `None`, otherwise,
/// if it's found the message and information it needs, it should return `Some(value)`, which will
/// be returned by this function as `Ok(value)`.
///
/// If anything goes wrong with decoding or the receiver, an `Err(String)` will be returned with an
/// error message. An `Err` will also be returned if the timeout is reached without receiving data
/// from the callback.
pub async fn await_mesh_response<T>(
    receiver: &mut tokio::sync::broadcast::Receiver<bytes::Bytes>,
    timeout_duration: Duration,
    mut callback: impl FnMut(CrisislabMessage) -> Option<T>,
) -> Result<T, String> {
    tokio::time::timeout(timeout_duration, async {
        loop {
            match receiver.recv().await {
                Ok(buffer) => match CrisislabMessage::decode(buffer) {
                    Ok(message) => {
                        let result = callback(message);
                        if let Some(value) = result {
                            return Ok(value);
                        }
                    }
                    Err(error) => {
                        return Err(format!("Failed to decode CrisislabMessage: {:?}", error));
                    }
                },
                Err(RecvError::Lagged(_)) => {
                    return Err("Mesh response receiver lagged".to_string());
                }
                Err(RecvError::Closed) => {
                    return Err("Mesh response receiver closed".to_string());
                }
            };
        }
    })
    .await
    .unwrap_or(Err(format!(
        "Timed out waiting for mesh response after {} seconds",
        timeout_duration.as_secs()
    )))
}
