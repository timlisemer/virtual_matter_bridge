//! Test binary for W100 MQTT communication.
//!
//! Usage:
//!   cargo run --bin mqtt-test
//!
//! This connects to the MQTT broker, subscribes to W100 topics,
//! and logs state changes and button presses. It also tests
//! setting the external display values.

use log::{info, warn};
use std::sync::Arc;
use tokio::sync::mpsc;

// Import from the library crate
use virtual_matter_bridge::config::Config;
use virtual_matter_bridge::input::mqtt::{MqttClient, W100Action, W100Device};

#[tokio::main]
async fn main() {
    // Load .env file before anything else
    virtual_matter_bridge::config::load_dotenv();

    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting W100 MQTT test");

    // Load configuration
    let config = Config::from_env();
    info!(
        "Connecting to MQTT broker at {}:{}",
        config.mqtt.broker_host, config.mqtt.broker_port
    );

    // Create MQTT client
    let mqtt_client = MqttClient::new(&config.mqtt);
    let async_client = mqtt_client.client();

    // Create W100 device handler
    let (action_tx, mut action_rx) = mpsc::channel::<W100Action>(32);
    let w100 = Arc::new(
        W100Device::new("Tim-Thermometer")
            .with_mqtt_client(async_client.clone())
            .with_action_channel(action_tx),
    );

    // Subscribe to W100 topics
    for topic in w100.subscribe_topics() {
        if let Err(e) = mqtt_client.subscribe(&topic).await {
            warn!("Failed to subscribe to {}: {}", topic, e);
        }
    }

    // Create message channel
    let (msg_tx, mut msg_rx) = mpsc::channel(100);

    // Spawn MQTT event loop
    let mqtt_handle = tokio::spawn(async move {
        mqtt_client.run(msg_tx).await;
    });

    // Spawn message processor
    let w100_clone = w100.clone();
    let msg_handle = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            w100_clone.process_message(&msg.topic, &msg.payload).await;
        }
    });

    // Spawn action handler
    let action_handle = tokio::spawn(async move {
        while let Some(action) = action_rx.recv().await {
            match action {
                W100Action::SinglePlus => {
                    info!(">>> PLUS button pressed!");
                }
                W100Action::SingleMinus => {
                    info!(">>> MINUS button pressed!");
                }
                W100Action::SingleCenter => {
                    info!(">>> CENTER button pressed!");
                }
                _ => {
                    info!(">>> Button action: {:?}", action);
                }
            }
        }
    });

    // Wait a moment for connection
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Test: Set external temperature to 23.5
    info!("Testing: Setting external temperature to 23.5°C...");
    if let Err(e) = w100.set_external_temperature(23.5).await {
        warn!("Failed to set external temperature: {}", e);
    }

    // Wait and read current state
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    let state = w100.get_state().await;
    info!(
        "Current state: temp={:?}°C, humidity={:?}%",
        state.temperature, state.humidity
    );

    info!("Listening for button presses... Press Ctrl+C to exit.");
    info!("Try pressing +, -, or center button on the W100!");

    // Wait for tasks (they run indefinitely)
    tokio::select! {
        _ = mqtt_handle => {
            warn!("MQTT event loop ended");
        }
        _ = msg_handle => {
            warn!("Message processor ended");
        }
        _ = action_handle => {
            warn!("Action handler ended");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down...");
        }
    }

    // Reset to internal mode before exit
    info!("Resetting to internal sensor mode...");
    if let Err(e) = w100.set_internal_mode().await {
        warn!("Failed to reset to internal mode: {}", e);
    }

    info!("Test complete.");
}
