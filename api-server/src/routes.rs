use std::sync::Arc;

use crate::{
    mqtt::LoraGatewayInterface,
    proto::meshtastic::{crisislab_command, CrisislabCommand},
    utils::SimpleResponse,
};
use axum::{extract::State, http::StatusCode, Json};
use bytes::BytesMut;
use log::{debug, info};
use prost::Message;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SetBroadcastInterval {
    broadcast_interval_seconds: u32,
}

/// Returns a JSON object with an error message if something goes wrong, otherwise just a 200 OK
pub async fn set_broadcast_interval(
    State(mqtt_interface): State<Arc<impl LoraGatewayInterface>>,
    Json(body): Json<SetBroadcastInterval>,
) -> SimpleResponse {
    debug!(
        "Got request to set broadcast interval to {} seconds",
        body.broadcast_interval_seconds
    );

    let command = CrisislabCommand {
        r#type: crisislab_command::Type::SetBroadcastInterval.into(),
        payload: Some(crisislab_command::Payload::BroadcastIntervalSeconds(
            body.broadcast_interval_seconds,
        )),
    };

    let mut buffer = BytesMut::with_capacity(command.encoded_len());
    if let Err(error) = command.encode(&mut buffer) {
        return SimpleResponse::Err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to encode command: {:?}", error),
        )
        .log();
    }

    debug!("Encoded set broadcast interval command: {:?}", buffer);

    if let Err(error) = mqtt_interface
        .clone_sender_to_publisher()
        .send(("commands".to_string(), buffer.freeze()))
        .await
    {
        return SimpleResponse::Err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send command to MQTT publisher task: {:?}", error),
        )
        .log();
    }

    info!("Sent set broadcast interval command to MQTT publisher task");

    return SimpleResponse::Ok;
}
