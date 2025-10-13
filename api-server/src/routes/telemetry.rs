use std::{
    sync::atomic::Ordering,
    time::Duration,
};

use crate::{
    proto::meshtastic::{
        crisislab_message::{self, Telemetry},
        CrisislabMessage,
    },
    utils::{
        await_mesh_response, send_command_protobuf, BoundedVecDeque, StringOrEmptyResponse
    },
    AppState,
};
use axum::{
    extract::{ws::WebSocket, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
    Json,
};
use bytes::Bytes;
use log::{debug, error, info};
use prost::Message;
use serde::{Deserialize, Serialize};

pub async fn live_telemetry(
    websocket_upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    websocket_upgrade.on_upgrade(|socket| handle_live_telemetry_websocket(socket, state))
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum TelemetryWSPacket<'a> {
    Telemetry(&'a Telemetry),
    Cache(&'a BoundedVecDeque<Telemetry>),
    Error(String),
}

async fn on_message_from_mesh(websocket: &mut WebSocket, state: &AppState, bytes: Bytes) {
    match CrisislabMessage::decode(bytes) {
        Ok(crisislab_message) => {
            if let Some(crisislab_message::Message::Telemetry(live_data)) =
                crisislab_message.message
            {
                // stringify data and send to client on websocket
                if websocket
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&TelemetryWSPacket::Telemetry(&live_data))
                            .expect("Failed to serialize CrisislabMessage for WS message")
                            .into(),
                    ))
                    .await
                    .is_err()
                {
                    debug!("Client disconnected from websocket");
                    return;
                }

                state.telemetry_cache.lock().await.write(live_data);
            }
        }
        Err(error) => {
            error!("Failed to decode CrisislabMessage: {:?}", error);

            // notify client of decoding error

            let packet =
                TelemetryWSPacket::Error(format!("Failed to decode CrisislabMessage: {:?}", error));

            if websocket
                .send(axum::extract::ws::Message::Text(
                    serde_json::to_string(&packet)
                        .expect("Failed to serialize error packet to send to WS client")
                        .into(),
                ))
                .await
                .is_err()
            {
                error!("Failed to inform WS client of decoding error. Disconnecting.");
                return;
            }
        }
    }
}

pub async fn start_live_telemetry(State(state): State<AppState>) -> StringOrEmptyResponse {
    debug!("Received request to start live telemetry");

    let message = CrisislabMessage {
        message: Some(crisislab_message::Message::StartLiveTelemetry(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) = send_command_protobuf(message, &state.mesh_interface).await {
        StringOrEmptyResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log()
    } else {
        debug!("Sent StartLiveTelemetry message to mesh");

        state
            .live_telemetry_is_enabled
            .store(true, Ordering::Relaxed);

        StringOrEmptyResponse::Ok
    }
}

pub async fn stop_live_telemetry(State(state): State<AppState>) -> StringOrEmptyResponse {
    debug!("Received request to stop live telemetry");

    let message = CrisislabMessage {
        message: Some(crisislab_message::Message::StopLiveTelemetry(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) = send_command_protobuf(message, &state.mesh_interface).await {
        StringOrEmptyResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log()
    } else {
        debug!("Sent StopLiveTelemetry message to mesh");

        state
            .live_telemetry_is_enabled
            .store(false, Ordering::Relaxed);

        StringOrEmptyResponse::Ok
    }
}

#[derive(Serialize)]
pub struct LiveStatusResponse {
    is_enabled: bool,
}

pub async fn get_live_telemetry_status(State(state): State<AppState>) -> Json<LiveStatusResponse> {
    Json(LiveStatusResponse {
        is_enabled: state.live_telemetry_is_enabled.load(Ordering::Relaxed),
    })
}

async fn handle_live_telemetry_websocket(mut websocket: WebSocket, state: AppState) {
    info!("Client connected to live info websocket");

    // get recent telemetry and send to client

    let telemetry_cache = state.telemetry_cache.lock().await;

    let serialised_cache = serde_json::to_string(&TelemetryWSPacket::Cache(&*telemetry_cache))
        .expect("Failed to serialise telemetry cache");

    drop(telemetry_cache);

    if websocket
        .send(axum::extract::ws::Message::Text(serialised_cache.into()))
        .await
        .is_err()
    {
        error!("Failed to send recent telemetry to WS client. Disconnecting.");
        return;
    }

    // main loop which alternates between forwarding telemetry from the mesh and checking for
    // websocket disconnections

    loop {
        let mut mesh_receiver = state.mesh_interface.subscribe();

        // NOTE: splitting `websocket` and using two tasks here might be better but I'm not sure
        tokio::select! {
            // handler message from mesh
            Ok(bytes) = mesh_receiver.recv() => {
                on_message_from_mesh(&mut websocket, &state, bytes).await;
            }
            // handle disconnections
            websocket_message = websocket.recv() => {
                if websocket_message.is_none() || websocket_message.unwrap().is_err() {
                    debug!("Client disconnected from websocket");
                    return;
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAdHocTelemetryBody {
    node_id: u32,
}

pub async fn get_ad_hoc_telemetry(
    State(state): State<AppState>,
    Json(body): Json<GetAdHocTelemetryBody>,
) -> StringOrEmptyResponse {
    info!("Requesting ad hoc telemetry from node {}", body.node_id);

    let crisislab_message = CrisislabMessage {
        message: Some(crisislab_message::Message::GetAdHocTelemetry(body.node_id)),
    };

    if let Err(error_message) =
        send_command_protobuf(crisislab_message, &state.mesh_interface).await
    {
        return StringOrEmptyResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    let app_settings = state.app_settings.lock().await;

    let telemetry_result: Result<(), String> = await_mesh_response(
        &mut state.mesh_interface.subscribe(),
        Duration::from_secs(app_settings.ad_hoc_telemetry_timeout_seconds),
        |message| {
            if let Some(crisislab_message::Message::Telemetry(_)) = message.message {
                Some(())
            } else {
                None::<()>
            }
        },
    )
    .await;

    if telemetry_result.is_ok() {
        debug!("Detected telemetry packet in get_ad_hoc_telemetry");
        StringOrEmptyResponse::Ok
    } else {
        StringOrEmptyResponse::Err(
            StatusCode::GATEWAY_TIMEOUT,
            format!("Timed out waiting for telemetry packet. Consider increasing ad_hoc_telemetry_timeout_seconds if mesh traffic is high.")
        )
        .log()
    }
}
