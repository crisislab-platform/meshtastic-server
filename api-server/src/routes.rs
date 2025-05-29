use std::{collections::HashMap, future::Future, pin::Pin, time::Duration};

use crate::{
    pathfinding::{self, compute_edge_weight, AdjacencyMap, NodeId},
    proto::meshtastic::{crisislab_message, CrisislabMessage},
    utils::JsonAndStatusResponse,
    AppState, MeshInterface,
};
use axum::{
    extract::{ws::WebSocket, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
    Json,
};
use bytes::BytesMut;
use log::{debug, error};
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
    State<MeshInterface>,
    T,
) -> Pin<Box<dyn Future<Output = JsonAndStatusResponse<()>> + Send>>
       + Clone {
    move |State(mesh_interface): State<MeshInterface>, body: T| {
        Box::pin(async move {
            debug!("Running curried command handler");

            let message = crisislab_message_creator(body);

            if let Err(error_message) = send_command_protobuf(message, &mesh_interface).await {
                JsonAndStatusResponse::Err(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    error_message,
                )
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
    State<MeshInterface>,
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
    State<MeshInterface>,
    Json<SetChannelNameBody>,
) -> Pin<Box<dyn Future<Output = JsonAndStatusResponse<()>> + Send>>
       + Clone {
    create_command_handler(|Json(body): Json<SetChannelNameBody>| CrisislabMessage {
        message: Some(crisislab_message::Message::ChannelName(body.channel_name)),
    })
}

type RoutesUpdateResponse = pathfinding::Routes<NodeId>;

// /admin/update-routes
#[axum::debug_handler]
pub async fn update_routes(
    State(mesh_interface): State<MeshInterface>,
) -> JsonAndStatusResponse<RoutesUpdateResponse> {
    let update_routes_message = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdateRoutes(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) = send_command_protobuf(update_routes_message, &mesh_interface).await
    {
        return JsonAndStatusResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
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

    // let routes_message = CrisislabMessage {
    //     message: Some(crisislab_message::Message::updated_routes())
    // }

    JsonAndStatusResponse::Ok(routes)
}

pub async fn live_info(
    websocket_upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    websocket_upgrade.on_upgrade(|socket| handle_live_info_websocket(socket, state))
}

async fn handle_live_info_websocket(websocket: WebSocket, state: AppState) {
    while let Ok(message) = state.mesh_interface.subscribe().recv().await {
        debug!("Live info websocket is listening for data from the mesh");
        match CrisislabMessage::decode(message) {
            Ok(crisislab_message) => {
                // if let Some(crisislab_message::Message::LiveData)
            }
            Err(error) => {
                error!("Failed to decode CrisislabMessage: {:?}", error);
            }
        }
    }
}
