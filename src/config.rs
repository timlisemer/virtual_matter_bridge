use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Load environment variables from .env file with robust parsing.
/// Handles values with spaces without requiring quotes.
pub fn load_dotenv() {
    let env_path = Path::new(".env");
    if !env_path.exists() {
        return;
    }

    let content = match fs::read_to_string(env_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Find the first '=' and split there
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            let mut value = line[eq_pos + 1..].trim();

            // Remove surrounding quotes if present
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = &value[1..value.len() - 1];
            }

            // Only set if not already set (env vars take precedence)
            if std::env::var(key).is_err() {
                // SAFETY: We're single-threaded at this point (called before any async runtime)
                unsafe { std::env::set_var(key, value) };
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rtsp: RtspConfig,
    pub matter: MatterConfig,
    pub webrtc: WebRtcConfig,
    pub doorbell: DoorbellConfig,
    pub mqtt: MqttConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub client_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtspConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_name: String,
    pub discriminator: u16,
    pub passcode: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcConfig {
    pub stun_servers: Vec<String>,
    pub turn_servers: Vec<TurnServer>,
    pub max_concurrent_streams: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnServer {
    pub url: String,
    pub username: String,
    pub credential: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoorbellConfig {
    // Doorbell configuration (chime removed - handled by controller)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rtsp: RtspConfig {
                url: "rtsp://username:password@10.0.0.38:554/h264Preview_01_main".to_string(),
                username: Some("username".to_string()),
                password: Some("password".to_string()),
            },
            matter: MatterConfig {
                vendor_id: 0xFFF1,
                product_id: 0x8001,
                device_name: "Virtual Matter Bridge".to_string(),
                discriminator: 3840,
                passcode: 20202021,
            },
            webrtc: WebRtcConfig {
                stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
                turn_servers: vec![],
                max_concurrent_streams: 4,
            },
            doorbell: DoorbellConfig {},
            mqtt: MqttConfig {
                broker_host: "10.0.0.2".to_string(),
                broker_port: 1883,
                client_id: "virtual-matter-bridge".to_string(),
                username: None,
                password: None,
            },
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(url) = std::env::var("RTSP_URL") {
            config.rtsp.url = url;
        }
        if let Ok(username) = std::env::var("RTSP_USERNAME") {
            config.rtsp.username = Some(username);
        }
        if let Ok(password) = std::env::var("RTSP_PASSWORD") {
            config.rtsp.password = Some(password);
        }
        if let Ok(name) = std::env::var("DEVICE_NAME") {
            config.matter.device_name = name;
        }
        if let Ok(discriminator) = std::env::var("MATTER_DISCRIMINATOR")
            && let Ok(d) = discriminator.parse()
        {
            config.matter.discriminator = d;
        }
        if let Ok(passcode) = std::env::var("MATTER_PASSCODE")
            && let Ok(p) = passcode.parse()
        {
            config.matter.passcode = p;
        }

        // MQTT configuration
        if let Ok(host) = std::env::var("MQTT_BROKER_HOST") {
            config.mqtt.broker_host = host;
        }
        if let Ok(port) = std::env::var("MQTT_BROKER_PORT")
            && let Ok(p) = port.parse()
        {
            config.mqtt.broker_port = p;
        }
        if let Ok(client_id) = std::env::var("MQTT_CLIENT_ID") {
            config.mqtt.client_id = client_id;
        }
        if let Ok(username) = std::env::var("MQTT_USERNAME") {
            config.mqtt.username = Some(username);
        }
        if let Ok(password) = std::env::var("MQTT_PASSWORD") {
            config.mqtt.password = Some(password);
        }

        config
    }
}
