mod config;
mod mqtt;
mod pathfinding;
mod proto;
mod routes;
mod utils;

use axum::{
    extract::FromRef, http::{header::{AUTHORIZATION, CONTENT_TYPE}, HeaderValue, Method}, routing::{any, get, post}, Router
};
use bytes::Bytes;
use config::CONFIG;
use pathfinding::EdgeWeight;
use proto::meshtastic::crisislab_message::Telemetry;
use serde::Serialize;
use tower_http::cors::CorsLayer;
use std::sync::{atomic::AtomicUsize, Arc};
use tokio::sync::{broadcast, mpsc, Mutex};
use utils::RingBuffer;

/// Outer state struct to be passed to Axum handlers
#[derive(Clone)]
pub struct AppState {
    mesh_interface: MeshInterface,
    app_settings: Arc<Mutex<AppSettings>>,
    updating_routes_lock: Arc<Mutex<()>>,
    websocket_count: Arc<AtomicUsize>,
    telemetry_cache: Arc<Mutex<RingBuffer<Telemetry>>>,
}

/// Struct containing the two Tokio channels required for communication with the mesh
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

// These FromRef impls allow the outer AppState struct to be derferenced to inner components
impl FromRef<AppState> for MeshInterface {
    fn from_ref(app_state: &AppState) -> MeshInterface {
        app_state.mesh_interface.clone()
    }
}

/// Settings relating to the server not the mesh
#[derive(Clone, Serialize)]
pub struct AppSettings {
    get_settings_timeout_seconds: u64,
    signal_data_timeout_seconds: u64,
    route_cost_weight: EdgeWeight,
    route_hops_weight: EdgeWeight,
}

impl FromRef<AppState> for Arc<Mutex<AppSettings>> {
    fn from_ref(app_state: &AppState) -> Arc<Mutex<AppSettings>> {
        app_state.app_settings.clone()
    }
}

pub fn init_app(state: AppState) -> Router {
    // temporary cors fix for testing on Migada's laptop
    let allowlist = [
        HeaderValue::from_static("http://localhost:8000"),
        HeaderValue::from_static("http://127.0.0.1:8000"),
    ];

    let cors = CorsLayer::new()
        .allow_origin(allowlist)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION])
        .allow_credentials(true);

    Router::new()
        .route("/admin/set-mesh-settings", post(routes::set_mesh_settings))
        .route(
            "/admin/set-server-settings",
            post(routes::set_server_settings),
        )
        .route("/get-mesh-settings", get(routes::get_mesh_settings))
        .route("/get-server-settings", get(routes::get_server_settings))
        .route("/admin/update-routes", get(routes::update_routes))
        .route("/info/live", any(routes::live_info))
        .route("/info/ad-hoc", get(routes::get_ad_hoc_data))
        .layer(cors)
        .with_state(state)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    env_logger::init();

    let mesh_interface = mqtt::init_client().await;

    let app_state = AppState {
        mesh_interface,
        app_settings: Arc::new(Mutex::new(AppSettings {
            get_settings_timeout_seconds: CONFIG.default_get_settings_timeout_seconds,
            signal_data_timeout_seconds: CONFIG.default_signal_data_timeout_seconds,
            route_cost_weight: CONFIG.default_route_cost_weight,
            route_hops_weight: CONFIG.default_route_hops_weight,
        })),
        updating_routes_lock: Arc::new(Mutex::new(())),
        websocket_count: Arc::new(AtomicUsize::new(0)),
        telemetry_cache: Arc::new(Mutex::new(RingBuffer::new(CONFIG.telemetry_cache_capacity))),
    };

    let app = init_app(app_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", CONFIG.server_port))
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
