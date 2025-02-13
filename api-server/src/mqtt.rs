use crate::config::CONFIG;
use tokio::{sync::{broadcast, mpsc}, task::JoinHandle};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet};
use std::time::Duration;
use log::{debug, error, info};

pub type MqttPacket = (String, Vec<u8>); // topic and bytes

fn publisher_task(
    client: AsyncClient,
    mut rx: mpsc::Receiver<MqttPacket>
) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Starting MQTT publisher task");

        while let Some((topic, bytes)) = rx.recv().await {
            client.publish(topic, CONFIG.mqtt_qos, false, bytes.as_slice())
                .await.unwrap_or_else(|error| {
                    error!("Failed to publish MQTT message: {:?}", error);
                });
        }
    })
}

fn subscriber_task(
    mut event_loop: EventLoop,
    tx: broadcast::Sender<MqttPacket>
) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("Starting MQTT subscriber task");

        loop {
            match event_loop.poll().await {
                Ok(event) => {
                    if let Event::Incoming(Packet::Publish(packet)) = event {
                        debug!("Got message from MQTT on \"{}\" topic ({} bytes)", packet.topic, packet.payload.len());

                        if tx.receiver_count() == 0 {
                            debug!("No broadcast receivers, skipping broadcast");
                            continue;
                        }

                        tx.send((packet.topic, packet.payload.to_vec()))
                            .expect("Failed to broadcast MQTT message");
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

pub struct MqttTaskChannels {
    pub mpsc_tx: mpsc::Sender<MqttPacket>,
    pub broadcast_tx: broadcast::Sender<MqttPacket>
}

pub async fn init_client() -> MqttTaskChannels {
    let mut options = MqttOptions::new(
        "crisislab-api-server", CONFIG.mqtt_host.as_str(), CONFIG.mqtt_port
    );

    options.set_keep_alive(Duration::from_secs(30));
    options.set_credentials(CONFIG.mqtt_username.as_str(), CONFIG.mqtt_password.as_str());

    let (client, event_loop) = AsyncClient::new(options, CONFIG.channel_capacity);

    for topic in &CONFIG.mqtt_topics {
        client.subscribe(topic, CONFIG.mqtt_qos)
            .await.expect(&format!("Failed to subscribe to {} channel", topic));
    }

    let (mpsc_tx, mpsc_rx) = mpsc::channel::<MqttPacket>(CONFIG.channel_capacity);
    let (broadcast_tx, _) = broadcast::channel::<MqttPacket>(
        CONFIG.channel_capacity
    );

    publisher_task(client, mpsc_rx);
    subscriber_task(event_loop, broadcast_tx.clone());

    MqttTaskChannels { mpsc_tx, broadcast_tx }
}
