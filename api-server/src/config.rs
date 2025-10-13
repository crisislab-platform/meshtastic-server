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
    pub default_next_hops_per_node_per_gateway: usize,
    pub telemetry_cache_capacity: usize,
    pub default_ad_hoc_telemetry_timeout_seconds: u64,
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

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    if cfg!(test) {
        panic!("CONFIG should not be loaded in tests!");
    }

    let config = Config {
        mqtt_username: get_env_var("MQTT_USERNAME"),
        mqtt_password: get_env_var("MQTT_PASSWORD"),
        mqtt_host: get_env_var("MQTT_HOST"),
        mqtt_port: get_env_var("MQTT_PORT")
            .parse::<u16>()
            .expect("MQTT_PORT must be a u16"),
        mqtt_qos: qos_from_str(get_env_var("MQTT_QOS").as_str())
            .expect("MQTT_QOS is an invalid option"),
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
            .expect("DEFAULT_GET_SETTINGS_TIMEOUT_SECONDS must be a u64"),
        default_signal_data_timeout_seconds: get_env_var("DEFAULT_SIGNAL_DATA_TIMEOUT_SECONDS")
            .parse::<u64>()
            .expect("DEFAULT_SIGNAL_DATA_TIMEOUT_SECONDS must be a u64"),
        default_route_cost_weight: get_env_var("DEFAULT_ROUTE_COST_WEIGHT")
            .parse::<EdgeWeight>()
            .expect("DEFAULT_ROUTE_COST_WEIGHT must be an EdgeWeight"),
        default_route_hops_weight: get_env_var("DEFAULT_ROUTE_HOPS_WEIGHT")
            .parse::<EdgeWeight>()
            .expect("DEFAULT_ROUTE_HOPS_WEIGHT must be an EdgeWeight"),
        default_next_hops_per_node_per_gateway: get_env_var(
            "DEFAULT_NEXT_HOPS_PER_NODE_PER_GATEWAY",
        )
        .parse::<usize>()
        .expect("DEFAULT_NEXT_HOPS_PER_NODE_PER_GATEWAY must be a usize"),
        telemetry_cache_capacity: get_env_var("TELEMETRY_CACHE_CAPACITY")
            .parse::<usize>()
            .expect("TELEMETRY_CACHE_CAPACITY must be a usize"),
        default_ad_hoc_telemetry_timeout_seconds: get_env_var(
            "DEFAULT_AD_HOC_TELEMETRY_TIMEOUT_SECONDS",
        )
        .parse::<u64>()
        .expect("DEFAULT_AD_HOC_TELEMETRY_TIMEOUT_SECONDS must be a u64"),
    };

    if config.default_route_cost_weight < 0.0 || config.default_route_cost_weight > 1.0 {
        panic!("DEFAULT_ROUTE_COST_WEIGHT must be between 0.0 and 1.0");
    }

    if config.default_route_hops_weight < 0.0 || config.default_route_hops_weight > 1.0 {
        panic!("DEFAULT_ROUTE_HOPS_WEIGHT must be between 0.0 and 1.0");
    }

    if config.default_next_hops_per_node_per_gateway < 1 {
        panic!("DEFAULT_NEXT_HOPS_PER_NODE_PER_GATEWAY must be greater than zero");
    }

    config
});
