use once_cell::sync::Lazy;
use rumqttc::mqttbytes::QoS;

pub struct Config {
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_qos: QoS,
    pub mqtt_topics: Vec<String>,
    pub channel_capacity: usize
}

fn get_env_var(name: &str) -> String {
    std::env::var(name)
        .expect(&format!("Environment variable {} must be set", name))
}

fn qos_from_str(string: &str) -> Result<QoS, String> {
    match string {
        "AtMostOnce" => Ok(QoS::AtMostOnce),
        "AtLeastOnce" => Ok(QoS::AtLeastOnce),
        "ExactlyOnce" => Ok(QoS::ExactlyOnce),
        _ => Err(format!("Invalid QoS: {}", string))
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    Config {
        mqtt_username: get_env_var("MQTT_USERNAME"),
        mqtt_password: get_env_var("MQTT_PASSWORD"),
        mqtt_host: get_env_var("MQTT_HOST"),
        mqtt_port: get_env_var("MQTT_PORT")
            .parse::<u16>().expect("MQTT_PORT must be a u16"),
        mqtt_qos: qos_from_str(get_env_var("MQTT_QOS").as_str()).unwrap(),
        mqtt_topics: get_env_var("MQTT_TOPICS")
            .split(",")
            .map(|topic| topic.to_string())
            .collect(),
        channel_capacity: get_env_var("CHANNEL_CAPACITY")
            .parse::<usize>().expect("CHANNEL_CAPACITY must be a usize")
    }
});
