mod proto;
mod routes;
mod mqtt;
mod config;

use axum::{
    Router,
    routing::post
};
use log::{debug, info};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    env_logger::init();

    let mqtt_task_channels = mqtt::init_client().await;
    let mqtt_task_channels = Arc::new(mqtt_task_channels);

    let app = Router::new()
        .route("/set-broadcast-interval", post(routes::set_broadcast_interval))
        .with_state(mqtt_task_channels);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
