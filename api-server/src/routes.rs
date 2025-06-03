use std::{collections::HashMap, time::Duration};

use crate::{
    pathfinding::{self, compute_edge_weight, AdjacencyMap, NodeId},
    proto::meshtastic::{crisislab_message, CrisislabMessage},
    utils::JsonOrErrorResponse,
    AppSettings, AppState, MeshInterface,
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

// takes a CrisislabMessage, encodes it as a protobuf and sends it to the MQTT broker
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

pub async fn set_mesh_setting(
    State(mesh_interface): State<MeshInterface>,
    Json(body): Json<crisislab_message::MeshSettings>,
) -> JsonOrErrorResponse<()> {
    info!("Setting mesh settings: {:?}", body);

    let crisislab_message = CrisislabMessage {
        message: Some(crisislab_message::Message::MeshSettings(body)),
    };

    if let Err(error_message) = send_command_protobuf(crisislab_message, &mesh_interface).await {
        JsonOrErrorResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log()
    } else {
        JsonOrErrorResponse::Ok(())
    }
}

#[derive(Deserialize, Debug)]
pub struct ServerSettingsBody {
    signal_data_timeout_seconds: Option<u32>,
}

#[axum::debug_handler]
pub async fn set_server_setting(
    State(mut app_settings): State<AppSettings>,
    Json(body): Json<ServerSettingsBody>,
) -> StatusCode {
    info!("Setting server settings: {:?}", body);

    if let Some(signal_data_timeout_seconds) = body.signal_data_timeout_seconds {
        app_settings.signal_data_timeout_seconds = signal_data_timeout_seconds;
    }

    StatusCode::OK
}

type RoutesUpdateResponse = pathfinding::Routes<NodeId>;

// /admin/update-routes
#[axum::debug_handler]
pub async fn update_routes(
    State(mesh_interface): State<MeshInterface>,
) -> JsonOrErrorResponse<RoutesUpdateResponse> {
    let update_routes_message = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdateRoutesRequest(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) = send_command_protobuf(update_routes_message, &mesh_interface).await
    {
        return JsonOrErrorResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    let mut receiver = mesh_interface.subscribe();

    let mut adjacency_map: AdjacencyMap<NodeId> = HashMap::new();
    let mut gateway_ids = Vec::<NodeId>::new();

    let _ = tokio::time::timeout(Duration::from_secs(80), async {
        debug!("Update routes handler waiting for signal data...");

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

    let routes = pathfinding::compute_routes(adjacency_map, gateway_ids);

    // copy data from routes into the format expected by the protobuf
    let routes_message = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdatedRoutes(
            crisislab_message::RoutesMap {
                entries: routes.iter().map(|(start_node_id, paths)| {
                    (start_node_id.clone(), crisislab_message::routes_map::RoutesList {
                        routes: paths.iter().map(|path| {
                            crisislab_message::routes_map::Route {
                                node_ids: path.clone()
                            }
                        }).collect(),
                    })
                }).collect::<HashMap<_, _>>(),
            },
        )),
    };

    if let Err(error_message) = send_command_protobuf(routes_message, &mesh_interface).await {
        return JsonOrErrorResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    // return the more nicely fomatted routes to the web client
    JsonOrErrorResponse::Ok(routes)
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
