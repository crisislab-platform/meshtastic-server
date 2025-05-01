use std::{future::Future, pin::Pin, sync::Arc};

use crate::{
    mqtt::LoraGatewayInterface,
    proto::meshtastic::{crisislab_message, CrisislabMessage},
    utils::SimpleResponse,
};
use axum::{extract::State, http::StatusCode, Json};
use bytes::BytesMut;
use log::debug;
use prost::Message;
use serde::{de::DeserializeOwned, Deserialize};

/* This function returns an Axum handler for and endpoint for setting settings on every node in the
* mesh. If something goes wrong, an error will be returned in a JSON object, otherwise there will
* be no response body, just a 200 OK.
*
* In an attempt to answer some questions that this mildy concerning function signature may raise:
* - T must be DeserializeOwned to ensure it doesn't contain any references that could be outlived
* - T must be 'static since we return a pointer (whose referenced value could outlive this function)
* - You can't use impl Trait in the returned function's signature since it wouldn't make it
* concrete, hence the dyn on LoraGatewayInterface and the Future
* - Since the returned function returns a Future (which could be self-referential), we need to pin
* it to ensure those references remain valid
* - The returned function must be Clone to satisfy Axum's Handler trait
* */
fn create_setter_handler<T: DeserializeOwned + Send + 'static>(
    crisislab_message_creator: fn(T) -> CrisislabMessage,
) -> impl Fn(
    State<Arc<dyn LoraGatewayInterface>>,
    Json<T>,
) -> Pin<Box<dyn Future<Output = SimpleResponse> + Send>>
       + Clone {
    move |State(mqtt_interface): State<Arc<dyn LoraGatewayInterface>>, Json(body): Json<T>| {
        Box::pin(async move {
            debug!("Handling request to set some setting on all nodes");

            // create CrisislabMessage struct to be encoded into a protobuf
            let message = crisislab_message_creator(body);

            // buffer for the encoded protobuf
            let mut buffer = BytesMut::with_capacity(message.encoded_len());

            if let Err(error) = message.encode(&mut buffer) {
                return SimpleResponse::Err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to encode command: {:?}", error),
                )
                .log();
            }

            if let Err(error) = mqtt_interface
                // the Tokio channel sender which goes to the publisher task
                .clone_sender_to_publisher()
                // that channel expects a non-mutable Bytes buffer hence .freeze()
                .send(buffer.freeze())
                .await
            {
                return SimpleResponse::Err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to send command to MQTT publisher task: {:?}", error),
                )
                .log();
            }

            return SimpleResponse::Ok;
        })
    }
}

#[derive(Deserialize)]
pub struct SetBroadcastIntervalBody {
    broadcast_interval_seconds: u32,
}

// /admin/set-broadcast-interval
pub fn get_set_broadcast_interval_handler() -> impl Fn(
    State<Arc<dyn LoraGatewayInterface>>,
    Json<SetBroadcastIntervalBody>,
) -> Pin<Box<dyn Future<Output = SimpleResponse> + Send>>
       + Clone {
    create_setter_handler(|body: SetBroadcastIntervalBody| {
        CrisislabMessage {
            message: Some(crisislab_message::Message::BroadcastIntervalSeconds(
                body.broadcast_interval_seconds,
            )),
        }
    })
}

#[derive(Deserialize)]
pub struct SetChannelNameBody {
    channel_name: String,
}

// /admin/set-channel-name
pub fn get_set_channel_name_handler() -> impl Fn(
    State<Arc<dyn LoraGatewayInterface>>,
    Json<SetChannelNameBody>,
) -> Pin<Box<dyn Future<Output = SimpleResponse> + Send>>
       + Clone {
    create_setter_handler(|body: SetChannelNameBody| CrisislabMessage {
        message: Some(crisislab_message::Message::ChannelName(body.channel_name)),
    })
}
