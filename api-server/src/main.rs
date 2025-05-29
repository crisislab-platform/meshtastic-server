mod config;
mod mqtt;
mod pathfinding;
mod proto;
mod routes;
mod utils;

use axum::{extract::FromRef, routing::{any, post}, Router};
use bytes::Bytes;
use config::CONFIG;
use tokio::sync::{broadcast, mpsc};

#[derive(Clone)]
pub struct AppState {
    mesh_interface: MeshInterface,
}

#[derive(Clone)]
pub struct MeshInterface {
    sender_to_publisher: mpsc::Sender<Bytes>,
    sender_to_subscribers: broadcast::Sender<Bytes>,
}

impl MeshInterface {
    pub fn clone_sender_to_publisher(&self) -> mpsc::Sender<Bytes> {
        self.sender_to_publisher.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.sender_to_subscribers.subscribe()
    }
}

impl FromRef<AppState> for MeshInterface {
    fn from_ref(app_state: &AppState) -> MeshInterface {
        app_state.mesh_interface.clone()
    }
}

pub fn init_app(state: AppState) -> Router {
    Router::new()
        .route(
            "/admin/set-broadcast-interval",
            post(routes::get_set_broadcast_interval_handler()),
        )
        .route(
            "/admin/set-channel-name",
            post(routes::get_set_channel_name_handler()),
        )
        .route("/admin/update-routes", post(routes::update_routes))
        .route("/info/live", any(routes::live_info))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    env_logger::init();

    let mesh_interface = mqtt::init_client().await;
    let app_state = AppState { mesh_interface };
    let app = init_app(app_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", CONFIG.server_port))
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
