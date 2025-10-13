use std::{collections::HashMap, time::Duration};

use crate::{
    pathfinding::{self, compute_edge_weight_proportionalised, AdjacencyMap, NodeId},
    proto::meshtastic::{
        crisislab_message::{self},
        CrisislabMessage,
    },
    utils::{self, send_command_protobuf, FallibleJsonResponse},
    AppState,
};
use axum::{extract::State, http::StatusCode};
use log::debug;

type RoutesUpdateResponse = HashMap<NodeId, Vec<NodeId>>;

pub async fn update_next_hops(
    State(state): State<AppState>,
) -> FallibleJsonResponse<RoutesUpdateResponse> {
    let _guard = match state.updating_next_hops_lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            debug!(
                "Update next hops handler: already updating next hops, returning conflict response"
            );

            return FallibleJsonResponse::Err(
                StatusCode::CONFLICT,
                "Next hops update has already been requested by another client".to_owned(),
            );
        }
    };

    let update_next_hops = CrisislabMessage {
        message: Some(crisislab_message::Message::UpdateNextHopsRequest(
            crisislab_message::Empty {},
        )),
    };

    if let Err(error_message) = send_command_protobuf(update_next_hops, &state.mesh_interface).await
    {
        return FallibleJsonResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    debug!("Update next hops handler sent request to mesh");

    let mut adjacency_map: AdjacencyMap<NodeId> = HashMap::new();
    let mut gateway_ids = Vec::<NodeId>::new();

    let timeout_duration =
        Duration::from_secs(state.app_settings.lock().await.signal_data_timeout_seconds);

    debug!(
        "Update next hops handler waiting for signal data... (timeout after {:?})",
        timeout_duration
    );

    let _ = utils::await_mesh_response(
        &mut state.mesh_interface.subscribe(),
        timeout_duration,
        |message| {
            if let Some(crisislab_message::Message::SignalData(signal_data)) = message.message {
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
                    sub_map.insert(
                        edge.from,
                        compute_edge_weight_proportionalised(edge.rssi, edge.snr),
                    );
                }
            }

            None::<crisislab_message::SignalData>
        },
    )
    .await;

    debug!("Timeout reached for signal data, proceeding with pathfinding");

    let next_hops_map =
        pathfinding::compute_next_hops_map(state.app_settings, adjacency_map, gateway_ids).await;

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

    debug!("Update next hops handler completed (next hops have been sent to mesh), returning next hops to client now");

    FallibleJsonResponse::Ok(next_hops_map)
}
