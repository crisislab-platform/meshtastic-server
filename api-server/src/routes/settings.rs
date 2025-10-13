use std::{sync::Arc, time::Duration};

use crate::{
    pathfinding::EdgeWeight,
    proto::meshtastic::{
        crisislab_message::{self},
        CrisislabMessage,
    },
    utils::{self, send_command_protobuf, FallibleJsonResponse, StringOrEmptyResponse},
    AppSettings, AppState, MeshInterface,
};
use axum::{extract::State, http::StatusCode, Json};
use log::{debug, error, info};
use serde::Deserialize;
use tokio::sync::Mutex;

/// Structure that clients should send mesh settings in as JSON body
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MeshSettingsBody {
    channel_name: Option<String>,
    broadcast_interval_seconds: Option<u32>,
    ping_timeout_seconds: Option<u32>,
}

/// /admin/set-mesh-settings
pub async fn set_mesh_settings(
    State(mesh_interface): State<MeshInterface>,
    Json(body): Json<MeshSettingsBody>,
) -> StringOrEmptyResponse {
    info!("Setting mesh settings: {:?}", body);

    let crisislab_message = CrisislabMessage {
        message: Some(crisislab_message::Message::MeshSettings(
            crisislab_message::MeshSettings {
                channel_name: body.channel_name,
                broadcast_interval_seconds: body.broadcast_interval_seconds,
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

/// Structure that clients should send server settings in as JSON body
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ServerSettingsBody {
    get_settings_timeout_seconds: Option<u64>,
    signal_data_timeout_seconds: Option<u64>,
    route_cost_weight: Option<EdgeWeight>,
    route_hops_weight: Option<EdgeWeight>,
}

/// /admin/set-server-settings
pub async fn set_server_settings(
    State(state): State<AppState>,
    Json(body): Json<ServerSettingsBody>,
) -> StatusCode {
    info!("Setting server settings: {:?}", body);

    let mut app_settings = state.app_settings.lock().await;

    // for each setting, update it if it was given

    if let Some(get_settings_timeout_seconds) = body.get_settings_timeout_seconds {
        app_settings.get_settings_timeout_seconds = get_settings_timeout_seconds;
    }

    if let Some(signal_data_timeout_seconds) = body.signal_data_timeout_seconds {
        app_settings.signal_data_timeout_seconds = signal_data_timeout_seconds;
    }

    if let Some(route_cost_weight) = body.route_cost_weight {
        app_settings.route_cost_weight = route_cost_weight;
    }

    if let Some(route_hops_weight) = body.route_hops_weight {
        app_settings.route_hops_weight = route_hops_weight;
    }

    StatusCode::OK
}

/// /get-mesh-settings
pub async fn get_mesh_settings(
    State(state): State<AppState>,
) -> FallibleJsonResponse<crisislab_message::MeshSettings> {
    info!("Received request to get mesh settings");

    // message for mesh to request current mesh settings
    let request_message = CrisislabMessage {
        message: Some(crisislab_message::Message::GetMeshSettingsRequest(
            crisislab_message::Empty {},
        )),
    };

    // send request to the mesh to get the current mesh settings
    if let Err(error_message) = send_command_protobuf(request_message, &state.mesh_interface).await
    {
        return FallibleJsonResponse::Err(StatusCode::INTERNAL_SERVER_ERROR, error_message).log();
    }

    let timeout_duration =
        Duration::from_secs(state.app_settings.lock().await.get_settings_timeout_seconds);

    debug!(
        "Request for settings sent to mesh, waiting for response (timeout after {:?})",
        timeout_duration
    );

    // wait for some amount of time for the mesh to respond with a MeshSettings packet
    match utils::await_mesh_response(
        &mut state.mesh_interface.subscribe(),
        timeout_duration,
        |message| {
            if let Some(crisislab_message::Message::MeshSettings(mesh_settings)) = message.message {
                debug!("Received mesh settings: {:?}", mesh_settings);
                return Some(mesh_settings);
            }

            None::<crisislab_message::MeshSettings>
        },
    )
    .await
    {
        // yield the mesh settings if we received them
        Ok(mesh_settings) => FallibleJsonResponse::Ok(mesh_settings),
        // otherwise log and return an error
        Err(error_message) => {
            error!("Failed to receive mesh settings: {:?}", error_message);
            FallibleJsonResponse::Err(StatusCode::GATEWAY_TIMEOUT, error_message).log()
        }
    }
}

/// /get-server-settings
pub async fn get_server_settings(
    State(app_settings): State<Arc<Mutex<AppSettings>>>,
) -> Json<AppSettings> {
    Json(app_settings.lock().await.clone())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::proto::meshtastic::{crisislab_message, CrisislabMessage};
    use crate::routes::test_helpers::setup_test;

    mod set_mesh_settings {
        use super::*;

        #[tokio::test]
        async fn normal_case() {
            let (mut mock_mesh, server) = setup_test();

            let response = server
                .post("/admin/set-mesh-settings")
                .json(&json!({
                    "channel_name": "crisislab",
                    "broadcast_interval_seconds": 30,
                }))
                .await;

            response.assert_status_ok();
            response.assert_text("");

            let sent_to_mesh = mock_mesh.await_msg_for_mesh().await;

            assert_eq!(
                sent_to_mesh,
                CrisislabMessage {
                    message: Some(crisislab_message::Message::MeshSettings(
                        crisislab_message::MeshSettings {
                            channel_name: Some("crisislab".to_string()),
                            broadcast_interval_seconds: Some(30),
                            ping_timeout_seconds: None
                        },
                    )),
                }
            )
        }

        #[tokio::test]
        async fn invalid_setting() {
            let (mut mock_mesh, server) = setup_test();

            let response = server
                .post("/admin/set-mesh-settings")
                .json(&json!({
                    "broadcast_interval_seconds": 20,
                    "this_is_not_a_setting": true
                }))
                .await;

            response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
            response.assert_text_contains("Failed to deserialize the JSON body");

            mock_mesh.assert_no_msg();
        }
    }

    mod set_server_settings {
        use super::*;

        #[tokio::test]
        async fn normal_case() {
            let (mut mock_mesh, server) = setup_test();

            let response = server
                .post("/admin/set-server-settings")
                .json(&json!({
                    "get_settings_timeout_seconds": 15,
                    "route_cost_weight": 0.5,
                    "route_hops_weight": 0.5,
                }))
                .await;

            response.assert_status_ok();
            response.assert_text("");

            mock_mesh.assert_no_msg();
        }

        #[tokio::test]
        async fn invalid_setting() {
            let (mut mock_mesh, server) = setup_test();

            let response = server
                .post("/admin/set-server-settings")
                .json(&json!({
                    "this_is_not_a_setting": true,
                }))
                .await;

            response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
            response.assert_text_contains("Failed to deserialize the JSON body");

            mock_mesh.assert_no_msg();
        }
    }

    mod get_mesh_settings {
        use super::*;

        #[tokio::test]
        async fn normal_case() {
            let (mut mock_mesh, server) = setup_test();

            let http_response_handle =
                tokio::spawn(async move { server.get("/get-mesh-settings").await });

            assert_eq!(
                mock_mesh.await_msg_for_mesh().await,
                CrisislabMessage {
                    message: Some(crisislab_message::Message::GetMeshSettingsRequest(
                        crisislab_message::Empty {},
                    )),
                }
            );

            mock_mesh
                .send_from_mesh(CrisislabMessage {
                    message: Some(crisislab_message::Message::MeshSettings(
                        crisislab_message::MeshSettings {
                            channel_name: Some("awesome_channel_name".to_string()),
                            broadcast_interval_seconds: Some(30),
                            ping_timeout_seconds: Some(60),
                        },
                    )),
                })
                .await;

            let http_response = http_response_handle.await.unwrap();

            http_response.assert_status_ok();
            http_response.assert_json(&json!({
                "channel_name": "awesome_channel_name",
                "broadcast_interval_seconds": 30,
                "ping_timeout_seconds": 60,
            }));
        }

        #[tokio::test]
        async fn timeout_waiting_for_mesh() {
            let (mut mock_mesh, server) = setup_test();

            let http_response_handle =
                tokio::spawn(async move { server.get("/get-mesh-settings").await });

            assert_eq!(
                mock_mesh.await_msg_for_mesh().await,
                CrisislabMessage {
                    message: Some(crisislab_message::Message::GetMeshSettingsRequest(
                        crisislab_message::Empty {},
                    )),
                }
            );

            let http_response = http_response_handle.await.unwrap();

            http_response.assert_status(StatusCode::GATEWAY_TIMEOUT);

            let json = http_response.json::<serde_json::Value>();
            let object = json.as_object().expect("Response was not a JSON object");

            assert_eq!(object.len(), 1);
            assert!(object.contains_key("error"));
        }
    }

    mod get_server_settings {
        use super::*;

        #[tokio::test]
        async fn normal_case() {
            let (_, server) = setup_test();

            let response = server.get("/get-server-settings").await;

            response.assert_status_ok();
            response.assert_json(&json!({
                "get_settings_timeout_seconds": 1,
                "signal_data_timeout_seconds": 1,
                "route_cost_weight": 0.5,
                "route_hops_weight": 0.5,
                "ad_hoc_telemetry_timeout_seconds": 1,
            }));
        }

        // I genuinely can't think of a way this could fail...
    }
}
