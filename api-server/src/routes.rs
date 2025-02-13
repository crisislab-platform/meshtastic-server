use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use log::debug;
use serde::Deserialize;
use crate::{mqtt::MqttTaskChannels, proto::meshtastic::{crisislab_command, CrisislabCommand}};
use prost::Message;

#[derive(Deserialize)]
pub struct SetBroadcastInterval {
    broadcast_interval_seconds: u32,
}

pub async fn set_broadcast_interval(
    State(mqtt_task_channels): State<Arc<MqttTaskChannels>>,
    Json(body): Json<SetBroadcastInterval>
) -> impl IntoResponse {
    debug!("Got request to set broadcast interval to {} seconds", body.broadcast_interval_seconds);

    let command = CrisislabCommand {
        r#type: crisislab_command::Type::SetBroadcastInterval.into(),
        payload: Some(crisislab_command::Payload::BroadcastIntervalSeconds(
            body.broadcast_interval_seconds
        ))
    };

    let mut buffer = Vec::with_capacity(command.encoded_len());
    command.encode(&mut buffer) .expect("Failed to encode command protobuf");

    mqtt_task_channels.mpsc_tx.clone().send(("commands".to_string(), buffer))
        .await.expect("Failed to send command to MQTT task for publishing");

    "OK"
}
