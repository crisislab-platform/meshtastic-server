use crate::config::CONFIG;
use bytes::Bytes;
use log::{debug, error, info};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet};
use std::time::Duration;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

pub type MqttPacket = (String, Bytes); // topic and bytes

fn publisher_task(client: AsyncClient, mut rx: mpsc::Receiver<MqttPacket>) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Starting MQTT publisher task");

        // when we have a message on the mpsc channel, publish it to the MQTT broker
        while let Some((topic, bytes)) = rx.recv().await {
            client
                .publish(topic, CONFIG.mqtt_qos, false, bytes)
                .await
                .unwrap_or_else(|error| {
                    error!("Failed to publish MQTT message: {:?}", error);
                });
        }
    })
}

fn handle_mqtt_message(
    topic: String,
    payload: Bytes,
    tx_to_handlers: broadcast::Sender<MqttPacket>,
) {
    info!(
        "Got message from MQTT on \"{}\" topic ({} bytes)",
        topic,
        payload.len()
    );
}

fn subscriber_task(
    mut event_loop: EventLoop,
    tx_to_handlers: broadcast::Sender<MqttPacket>,
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

// dear future dev/maintainer/me: the following three blocks may look convoluted but it should make
// unit testing a lot easier since we can use dependency injection to run the server with a mock
// LoraGatewayInterface

pub trait LoraGatewayInterface: Send + Sync + 'static {
    fn clone_sender_to_publisher(&self) -> mpsc::Sender<MqttPacket>;
    fn subscribe(&self) -> broadcast::Receiver<MqttPacket>;
}

pub struct MqttInterface {
    sender_to_publisher: mpsc::Sender<MqttPacket>,
    sender_to_subscribers: broadcast::Sender<MqttPacket>,
}

impl LoraGatewayInterface for MqttInterface {
    fn clone_sender_to_publisher(&self) -> mpsc::Sender<MqttPacket> {
        self.sender_to_publisher.clone()
    }

    fn subscribe(&self) -> broadcast::Receiver<MqttPacket> {
        self.sender_to_subscribers.subscribe()
    }
}

pub async fn init_client() -> MqttInterface {
    let mut options = MqttOptions::new(
        "crisislab-api-server",
        CONFIG.mqtt_host.as_str(),
        CONFIG.mqtt_port,
    );

    options.set_keep_alive(Duration::from_secs(30));
    options.set_credentials(CONFIG.mqtt_username.as_str(), CONFIG.mqtt_password.as_str());

    let (client, event_loop) = AsyncClient::new(options, CONFIG.channel_capacity);

    for topic in &CONFIG.mqtt_topics {
        client
            .subscribe(topic, CONFIG.mqtt_qos)
            .await
            .expect(&format!("Failed to subscribe to {} channel", topic));
    }

    // channel for sending message from the mqtt subscriber task to all the endpoint handlers
    let (sender_to_publisher, outgoing_msg_receiver) =
        mpsc::channel::<MqttPacket>(CONFIG.channel_capacity);

    // channel for endpoint handlers to send message to the mqtt publisher task
    let (sender_to_subscribers, _) = broadcast::channel::<MqttPacket>(CONFIG.channel_capacity);

    publisher_task(client, outgoing_msg_receiver);

    // we need to clone the broadcast transmitter because it's being returned
    // so that .subscribe() can be called on it to create a receiver
    subscriber_task(event_loop, sender_to_subscribers.clone());

    MqttInterface {
        sender_to_publisher,
        sender_to_subscribers,
    }
}
