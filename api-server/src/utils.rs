use bytes::BytesMut;
use std::collections::VecDeque;
use std::time::Duration;

use axum::{http::StatusCode, response::IntoResponse, Json};
use log::{debug, error};
use prost::Message;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;

use crate::proto::meshtastic::CrisislabMessage;
use crate::MeshInterface;

#[derive(Serialize)]
// make this is serialised as if it's only the buffer field
#[serde(transparent)]
pub struct BoundedVecDeque<T> {
    buffer: VecDeque<T>,
    #[serde(skip)]
    max_len: usize,
}

impl<T> BoundedVecDeque<T> {
    pub fn new(max_len: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_len),
            max_len,
        }
    }

    pub fn write(&mut self, item: T) {
        if self.buffer.len() == self.max_len {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    pub fn resize(&mut self, new_capacity: usize) {
        if new_capacity >= self.max_len {
            // if we're growing the array, we simply extend the capacity
            self.buffer
                .reserve_exact(new_capacity - self.buffer.len());
        } else {
            if self.buffer.len() > new_capacity {
                self.buffer.drain(0..self.buffer.len() - new_capacity);
            }

            if self.buffer.len() < self.buffer.capacity() {
                let mut new_buffer = VecDeque::with_capacity(new_capacity);
                new_buffer.append(&mut self.buffer);
                self.buffer = new_buffer;
            }
        }
    }

    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}

// allows the ring buffer to be converted into an iterator starting at the first/oldest item

impl<'a, T> IntoIterator for &'a BoundedVecDeque<T> {
    type Item = &'a T;
    type IntoIter = std::collections::vec_deque::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter()
    }
}

/// Wrapper struct that allows an iterator to serialised
// pub struct SerializableIterator<'a, T: Serialize + 'a, I: Iterator<Item = &'a T> + Clone>(pub I);
//
// impl<'a, T, I> Serialize for SerializableIterator<'a, T, I>
// where
//     I: Iterator<Item = &'a T> + Clone,
//     T: serde::ser::Serialize + 'a,
// {
//     fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
//         let mut seq = serializer.serialize_seq(None)?;
//
//         for item in self.0.clone() {
//             seq.serialize_element(item)?;
//         }
//
//         seq.end()
//     }
// }

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

/// Encodes a given CrisislabMessage and sends it to the Tokio task responsible for publishing
/// messages to the MQTT broker. May return an `Err(String)` if encoding or sending fails.
pub async fn send_command_protobuf(
    message: CrisislabMessage,
    mesh_interface: &MeshInterface,
) -> Result<(), String> {
    // buffer for the encoded protobuf
    let mut buffer = BytesMut::with_capacity(message.encoded_len());

    if let Err(error) = message.encode(&mut buffer) {
        return Err(format!("Failed to encode command as protobuf: {:?}", error));
    }

    if let Err(error) = mesh_interface
        // the Tokio channel sender which goes to the publisher task
        .clone_sender_to_publisher()
        // that channel expects a non-mutable Bytes buffer hence .freeze()
        .send(buffer.freeze())
        .await
    {
        Err(format!(
            "Failed to send command to MQTT publisher task: {:?}",
            error
        ))
    } else {
        debug!("send_command_protobuf: sent message to MQTT publisher task");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    mod bounded_vec_deque_tests {
        use super::super::BoundedVecDeque;

        #[test]
        fn creation() {
            let items = BoundedVecDeque::<usize>::new(3);
            assert_eq!(items.capacity(), 3);
        }

        #[test]
        fn write() {
            let mut items = BoundedVecDeque::<usize>::new(2);

            items.write(5);
            items.write(6);
            items.write(7);
            assert_eq!(items.capacity(), 2);
            assert!(items.into_iter().eq([6, 7].iter()));
        }

        #[test]
        fn resize_empty() {
            let mut items = BoundedVecDeque::<usize>::new(3);

            items.resize(5);
            assert_eq!(items.capacity(), 5);

            items.resize(2);
            assert_eq!(items.capacity(), 2);

            items.resize(0);
            assert_eq!(items.capacity(), 0);
        }

        #[test]
        fn shrink_full() {
            let mut items = BoundedVecDeque::<usize>::new(5);

            items.write(1);
            items.write(2);
            items.write(3);
            items.write(4);
            items.write(5);

            items.resize(3);
            assert_eq!(items.capacity(), 3);
            assert!(items.into_iter().eq([3, 4, 5].iter()));
        }

        #[test]
        fn expand_full() {
            let mut items = BoundedVecDeque::<usize>::new(3);

            items.write(1);
            items.write(2);
            items.write(3);

            items.resize(5);
            assert_eq!(items.capacity(), 5);
            assert!(items.into_iter().eq([1, 2, 3].iter()));
        }

        #[test]
        fn shrink_not_full_and_drop() {
            let mut items = BoundedVecDeque::<usize>::new(6);

            items.write(1);
            items.write(2);
            items.write(3);
            items.write(4);

            items.resize(3);
            assert_eq!(items.capacity(), 3);
            assert!(items.into_iter().eq([2, 3, 4].iter()));
        }

        #[test]
        fn shrink_not_full_no_drop() {
            let mut items = BoundedVecDeque::<usize>::new(6);

            items.write(1);
            items.write(2);
            items.write(3);
            items.write(4);

            items.resize(5);
            assert_eq!(items.capacity(), 5);
            assert!(items.into_iter().eq([1, 2, 3, 4].iter()));
        }

        #[test]
        fn expand_not_full() {
            let mut items = BoundedVecDeque::<usize>::new(3);

            items.write(1);
            items.write(2);

            items.resize(4);
            assert_eq!(items.capacity(), 4);
            assert!(items.into_iter().eq([1, 2].iter()));
        }
    }
}
