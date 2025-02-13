mod proto;
mod routes;
mod mqtt;
mod config;

use axum::{
    Router,
    routing::post
};
use config::CONFIG;
use log::debug;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    env_logger::init();

    let mqtt_task_channels = mqtt::init_client().await;

    let app = Router::new()
        .route("/set-broadcast-interval", post(routes::set_broadcast_interval))
        .with_state(Arc::new(mqtt_task_channels));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", CONFIG.server_port))
        .await.unwrap();
    axum::serve(listener, app).await.unwrap();

    debug!("Server started on port {}", CONFIG.server_port);
}
