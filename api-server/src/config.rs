use once_cell::sync::Lazy;
use rumqttc::mqttbytes::QoS;

use crate::pathfinding::EdgeWeight;

pub struct Config {
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_qos: QoS,
    pub mqtt_outgoing_topic: String,
    pub mqtt_incoming_topic: String,
    pub channel_capacity: usize,
    pub server_port: u16,
    pub default_get_settings_timeout_seconds: u64,
    pub default_signal_data_timeout_seconds: u64,
    pub default_route_cost_weight: EdgeWeight,
    pub default_route_hops_weight: EdgeWeight,
}

fn get_env_var(name: &str) -> String {
    std::env::var(name).expect(&format!("Environment variable {}", name))
}

fn qos_from_str(string: &str) -> Result<QoS, String> {
    match string {
        "AtMostOnce" => Ok(QoS::AtMostOnce),
        "AtLeastOnce" => Ok(QoS::AtLeastOnce),
        "ExactlyOnce" => Ok(QoS::ExactlyOnce),
        _ => Err(format!("Invalid QoS: {}", string)),
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config {
    mqtt_username: get_env_var("MQTT_USERNAME"),
    mqtt_password: get_env_var("MQTT_PASSWORD"),
    mqtt_host: get_env_var("MQTT_HOST"),
    mqtt_port: get_env_var("MQTT_PORT")
        .parse::<u16>()
        .expect("MQTT_PORT must be a u16"),
    mqtt_qos: qos_from_str(get_env_var("MQTT_QOS").as_str()).unwrap(),
    mqtt_outgoing_topic: get_env_var("MQTT_OUTGOING_TOPIC"),
    mqtt_incoming_topic: get_env_var("MQTT_INCOMING_TOPIC"),
    channel_capacity: get_env_var("CHANNEL_CAPACITY")
        .parse::<usize>()
        .expect("CHANNEL_CAPACITY must be a usize"),
    server_port: get_env_var("SERVER_PORT")
        .parse::<u16>()
        .expect("SERVER_PORT must be a u16"),
    default_get_settings_timeout_seconds: get_env_var("DEFAULT_GET_SETTINGS_TIMEOUT_SECONDS")
        .parse::<u64>()
        .expect("DEFAULT_GET_SETTINGS_TIMEOUT_SECONDS must be a u32"),
    default_signal_data_timeout_seconds: get_env_var("DEFAULT_SIGNAL_DATA_TIMEOUT_SECONDS")
        .parse::<u64>()
        .expect("DEFAULT_SIGNAL_DATA_TIMEOUT_SECONDS must be a u32"),
    default_route_cost_weight: get_env_var("DEFAULT_ROUTE_COST_WEIGHT")
        .parse::<EdgeWeight>()
        .expect("DEFAULT_ROUTE_COST_WEIGHT must be an EdgeWeight"),
    default_route_hops_weight: get_env_var("DEFAULT_ROUTE_HOPS_WEIGHT")
        .parse::<EdgeWeight>()
        .expect("DEFAULT_ROUTE_HOPS_WEIGHT must be an EdgeWeight"),
});
