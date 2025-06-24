use std::{collections::HashMap, time::Duration};

use crate::{
    pathfinding::{self, compute_edge_weight, AdjacencyMap, NodeId},
    proto::meshtastic::{crisislab_message, CrisislabMessage},
    utils::{FallibleJsonResponse, StringOrEmptyResponse},
    AppState, MeshInterface,
};
use axum::{
    extract::{ws::WebSocket, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
    Json,
};
use bytes::BytesMut;
use log::{debug, error, info};
use prost::Message;
use serde::Deserialize;

// encodes a given CrisislabMessage and sends to the Tokio task responsible for publishing message
// to the MQTT broker
async fn send_command_protobuf(
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

#[derive(Deserialize, Debug)]
pub struct MeshSettingsBody {
    broadcast_interval_seconds: Option<u32>,
    channel_name: Option<String>,
    ping_timeout_seconds: Option<u32>,
}

pub async fn set_mesh_settings(
    State(mesh_interface): State<MeshInterface>,
    Json(body): Json<MeshSettingsBody>,
) -> StringOrEmptyResponse {
    info!("Setting mesh settings: {:?}", body);

    let crisislab_message = CrisislabMessage {
        message: Some(crisislab_message::Message::MeshSettings(
            crisislab_message::MeshSettings {
                broadcast_interval_seconds: body.broadcast_interval_seconds,
                channel_name: body.channel_name,
                ping_timeout_seconds: body.ping_timeout_seconds,
            },
        )),
    };

    if let Err(error_message) = send_command_protobuf(crisislab_message, &mesh_interface).await {
        StringOrEmptyResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log()
    } else {
        StringOrEmptyResponse::Ok
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ServerSettingsBody {
    signal_data_timeout_seconds: Option<u32>,
}

pub async fn set_server_settings(
    State(state): State<AppState>,
    Json(body): Json<ServerSettingsBody>,
) -> StatusCode {
    info!("Setting server settings: {:?}", body);

    let mut app_settings = state.app_settings.lock().await;

    if let Some(signal_data_timeout_seconds) = body.signal_data_timeout_seconds {
        app_settings.signal_data_timeout_seconds = signal_data_timeout_seconds;
    }

    StatusCode::OK
}

type RoutesUpdateResponse = HashMap<NodeId, Vec<NodeId>>;

// /admin/update-routes
pub async fn update_routes(
    State(state): State<AppState>,
) -> FallibleJsonResponse<RoutesUpdateResponse> {
    let update_routes_message = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdateNextHopsRequest(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) =
        send_command_protobuf(update_routes_message, &state.mesh_interface).await
    {
        return FallibleJsonResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    debug!("Update routes handler sent request to mesh");

    let mut receiver = state.mesh_interface.subscribe();

    let mut adjacency_map: AdjacencyMap<NodeId> = HashMap::new();
    let mut gateway_ids = Vec::<NodeId>::new();

    let timeout_duration = Duration::from_secs(
        state
            .app_settings
            .lock()
            .await
            .signal_data_timeout_seconds
            .into(),
    );

    let _ = tokio::time::timeout(timeout_duration, async {
        debug!(
            "Update routes handler waiting for signal data... (timeout after {:?})",
            timeout_duration
        );

        while let Ok(buffer) = receiver.recv().await {
            match CrisislabMessage::decode(buffer) {
                Ok(message) => {
                    if let Some(crisislab_message::Message::SignalData(signal_data)) =
                        message.message
                    {
                        debug!("Signal data: {:?}", signal_data);

                        if signal_data.is_gateway {
                            gateway_ids.push(signal_data.to);
                        }

                        // get the map within the main ajacency map that we're going to fill
                        let sub_map = match adjacency_map.get_mut(&signal_data.to) {
                            Some(sub_map) => sub_map,
                            None => {
                                adjacency_map.insert(signal_data.to, HashMap::new());
                                adjacency_map.get_mut(&signal_data.to).unwrap()
                            }
                        };

                        for edge in signal_data.links {
                            sub_map.insert(edge.from, compute_edge_weight(edge.rssi, edge.snr));
                        }
                    }
                }
                Err(error) => {
                    error!("Failed to decode CrisislabMessage: {:?}", error);
                }
            }
        }
    })
    .await;

    let next_hops_map = pathfinding::compute_next_hops_map(adjacency_map, gateway_ids);

    debug!("Computed next hops map: {:?}", next_hops_map);

    let next_hops_message = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdatedNextHops(
            crisislab_message::NextHopsMap {
                entries: next_hops_map
                    .clone()
                    .into_iter()
                    .map(|(node_id, next_hops)| {
                        (
                            node_id,
                            crisislab_message::NextHops {
                                node_ids: next_hops,
                            },
                        )
                    })
                    .collect(),
            },
        )),
    };

    if let Err(error_message) =
        send_command_protobuf(next_hops_message, &state.mesh_interface).await
    {
        return FallibleJsonResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    debug!("Update routes handler completed (next hops have been sent to mesh), returning next hops to client now");

    FallibleJsonResponse::Ok(next_hops_map)
}

pub async fn live_info(
    websocket_upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    websocket_upgrade.on_upgrade(|socket| handle_live_info_websocket(socket, state))
}

async fn handle_live_info_websocket(mut websocket: WebSocket, state: AppState) {
    info!("Client connected live info websocket");

    while let Ok(message) = state.mesh_interface.subscribe().recv().await {
        debug!("Live info websocket is listening for data from the mesh");

        match CrisislabMessage::decode(message) {
            Ok(crisislab_message) => {
                if websocket
                    .send(axum::extract::ws::Message::Text(
                        serde_json::to_string(&crisislab_message)
                            .expect("Failed to serialize CrisislabMessage for WS message")
                            .into(),
                    ))
                    .await
                    .is_err()
                {
                    debug!("Client closed WS connection, closing handler");
                    return;
                }
            }
            Err(error) => {
                error!("Failed to decode CrisislabMessage: {:?}", error);
            }
        }
    }
}
