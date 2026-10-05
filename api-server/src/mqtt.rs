use crate::{config::CONFIG, MeshInterface};
use bytes::Bytes;
use log::{debug, error};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet};
use std::time::Duration;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

fn publisher_task(client: AsyncClient, mut rx: mpsc::Receiver<Bytes>) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Starting MQTT publisher task");

        // when we have a message on the mpsc channel, publish it to the MQTT broker
        while let Some(bytes) = rx.recv().await {
            client
                .publish(
                    CONFIG.mqtt_outgoing_topic.clone(),
                    CONFIG.mqtt_qos,
                    false,
                    bytes,
                )
                .await
                .unwrap_or_else(|error| {
                    error!("Failed to publish MQTT message: {:?}", error);
                });
        }
    })
}

#[allow(unused_variables)]
fn handle_mqtt_message(topic: String, payload: Bytes, tx_to_handlers: broadcast::Sender<Bytes>) {
    debug!(
        "Got message from MQTT on \"{}\" topic ({} bytes)",
        topic,
        payload.len()
    );

    // this logic might become more complex in the future
    if let Err(error) = tx_to_handlers.send(payload) {
        error!("Failed to send message to channel receivers. (No receivers?)");
    }
}

fn subscriber_task(
    client: AsyncClient,
    mut event_loop: EventLoop,
    tx_to_handlers: broadcast::Sender<Bytes>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Starting MQTT subscriber task");

        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    // rumqttc reconnects with clean_session=true (the crate default, and we
                    // don't override it), so the broker forgets our subscription on every
                    // reconnect - (re-)subscribe every time we see a fresh ConnAck, not just
                    // once at startup, or the server silently goes deaf after the first
                    // network blip / broker restart / keepalive timeout.
                    if let Err(error) = client
                        .subscribe(CONFIG.mqtt_incoming_topic.clone(), CONFIG.mqtt_qos)
                        .await
                    {
                        error!("Failed to (re-)subscribe to {}: {:?}", CONFIG.mqtt_incoming_topic, error);
                    } else {
                        debug!("(Re-)subscribed to {}", CONFIG.mqtt_incoming_topic);
                    }
                }
                Ok(Event::Incoming(Packet::Publish(packet))) => {
                    handle_mqtt_message(packet.topic, packet.payload, tx_to_handlers.clone());
                }
                Ok(_) => {}
                Err(error) => {
                    error!("Error polling MQTT event loop: {:?}", error);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    })
}

pub async fn init_client() -> MeshInterface {
    let mut options = MqttOptions::new(
        "crisislab-api-server",
        CONFIG.mqtt_host.as_str(),
        CONFIG.mqtt_port,
    );

    options.set_keep_alive(Duration::from_secs(30));
    options.set_credentials(CONFIG.mqtt_username.as_str(), CONFIG.mqtt_password.as_str());

    let (client, event_loop) = AsyncClient::new(options, CONFIG.channel_capacity);

    // Subscribing now happens inside subscriber_task on every ConnAck (including the
    // first), instead of once here - see subscriber_task for why.

    // channel for sending message from the mqtt subscriber task to all the endpoint handlers
    let (sender_to_publisher, outgoing_msg_receiver) =
        mpsc::channel::<Bytes>(CONFIG.channel_capacity);

    // channel for endpoint handlers to send message to the mqtt publisher task
    let (sender_to_subscribers, _) = broadcast::channel::<Bytes>(CONFIG.channel_capacity);

    publisher_task(client.clone(), outgoing_msg_receiver);

    // we need to clone the broadcast transmitter because it's being returned
    // so that .subscribe() can be called on it to create a receiver
    subscriber_task(client, event_loop, sender_to_subscribers.clone());

    MeshInterface {
        sender_to_publisher,
        sender_to_subscribers,
    }
}
