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
    mut event_loop: EventLoop,
    tx_to_handlers: broadcast::Sender<Bytes>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Starting MQTT subscriber task");

        loop {
            match event_loop.poll().await {
                Ok(event) => {
                    // for every message being received from the broker
                    if let Event::Incoming(Packet::Publish(packet)) = event {
                        handle_mqtt_message(packet.topic, packet.payload, tx_to_handlers.clone());
                    }
                }
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

    client
        .subscribe(CONFIG.mqtt_incoming_topic.clone(), CONFIG.mqtt_qos)
        .await
        .expect(&format!(
            "Failed to subscribe to {} channel",
            CONFIG.mqtt_incoming_topic
        ));

    // channel for sending message from the mqtt subscriber task to all the endpoint handlers
    let (sender_to_publisher, outgoing_msg_receiver) =
        mpsc::channel::<Bytes>(CONFIG.channel_capacity);

    // channel for endpoint handlers to send message to the mqtt publisher task
    let (sender_to_subscribers, _) = broadcast::channel::<Bytes>(CONFIG.channel_capacity);

    publisher_task(client, outgoing_msg_receiver);

    // we need to clone the broadcast transmitter because it's being returned
    // so that .subscribe() can be called on it to create a receiver
    subscriber_task(event_loop, sender_to_subscribers.clone());

    MeshInterface {
        sender_to_publisher,
        sender_to_subscribers,
    }
}
