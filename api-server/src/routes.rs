use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use crate::{
    mqtt::LoraGatewayInterface,
    proto::meshtastic::{crisislab_message, CrisislabMessage},
    utils::JsonAndStatusResponse,
};
use axum::{extract::State, http::StatusCode, Json};
use bytes::BytesMut;
use log::debug;
use prost::Message;
use serde::{Deserialize, Serialize};

// takes a CrisislabMessage, encodes it as a protobuf and sends it to the MQTT broker
async fn send_command_protobuf(
    message: CrisislabMessage,
    State(mqtt_interface): State<Arc<dyn LoraGatewayInterface>>,
) -> Result<(), String> {
    // buffer for the encoded protobuf
    let mut buffer = BytesMut::with_capacity(message.encoded_len());

    if let Err(error) = message.encode(&mut buffer) {
        return Err(format!("Failed to encode command as protobuf: {:?}", error));
    }

    if let Err(error) = mqtt_interface
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
        Ok(())
    }
}

/* This function returns an Axum handler for and endpoint which simply encodes a protobuf based on
* a request body and forwards it to the mesh over MQTT. If something goes wrong, an error will be
* returned in a JSON object, otherwise there will be no response body, just a 200 OK.
*
* In an attempt to answer some questions that this mildy concerning function signature may raise:
* - T is the type of the request body
* - T must be DeserializeOwned to ensure it doesn't contain any references that could be outlived
* - T must be 'static since we return a pointer (whose referenced value could outlive this function)
* - You can't use impl Trait in the returned function's signature since it wouldn't make it
* concrete, hence the dyn on LoraGatewayInterface and the Future
* - Since the returned function returns a Future (which could be self-referential), we need to pin
* it to ensure those references remain valid
* - The returned function must be Clone to satisfy Axum's Handler trait
* */
fn create_command_handler<T: Send + 'static>(
    crisislab_message_creator: fn(T) -> CrisislabMessage,
) -> impl Fn(
    State<Arc<dyn LoraGatewayInterface>>,
    T,
) -> Pin<Box<dyn Future<Output = JsonAndStatusResponse<()>> + Send>>
       + Clone {
    move |mqtt_interface: State<Arc<dyn LoraGatewayInterface>>, body: T| {
        Box::pin(async move {
            debug!("Running curried command handler");

            let message = crisislab_message_creator(body);

            if let Err(error_message) = send_command_protobuf(message, mqtt_interface).await {
                JsonAndStatusResponse::Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error_message)
                    .log()
            } else {
                JsonAndStatusResponse::Ok(())
            }
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
) -> Pin<Box<dyn Future<Output = JsonAndStatusResponse<()>> + Send>>
       + Clone {
    create_command_handler(
        |Json(body): Json<SetBroadcastIntervalBody>| CrisislabMessage {
            message: Some(crisislab_message::Message::BroadcastIntervalSeconds(
                body.broadcast_interval_seconds,
            )),
        },
    )
}

#[derive(Deserialize)]
pub struct SetChannelNameBody {
    channel_name: String,
}

// /admin/set-channel-name
pub fn get_set_channel_name_handler() -> impl Fn(
    State<Arc<dyn LoraGatewayInterface>>,
    Json<SetChannelNameBody>,
) -> Pin<Box<dyn Future<Output = JsonAndStatusResponse<()>> + Send>>
       + Clone {
    create_command_handler(|Json(body): Json<SetChannelNameBody>| CrisislabMessage {
        message: Some(crisislab_message::Message::ChannelName(body.channel_name)),
    })
}

#[derive(Deserialize, Serialize)]
struct GatewayCount {
    reachable: u8,
    diff: u8,
}

#[derive(Deserialize, Serialize)]
pub struct RoutesUpdate {
    routes: Vec<Vec<u32>>,
    diff: HashMap<u32, GatewayCount>,
}

// /admin/update-routes
pub async fn update_routes(
    mqtt_interface: State<Arc<dyn LoraGatewayInterface>>,
) -> JsonAndStatusResponse<RoutesUpdate> {
    let update_routes_message = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdateRoutes(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) = send_command_protobuf(update_routes_message, mqtt_interface).await {
        return JsonAndStatusResponse::Err(
            StatusCode::INTERNAL_SERVER_ERROR,
            error_message,
        )
        .log();
    }

    // TEMP
    JsonAndStatusResponse::Ok(RoutesUpdate {
        routes: vec![],
        diff: HashMap::new(),
    })
}
