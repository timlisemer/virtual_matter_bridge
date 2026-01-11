//! MQTT client wrapper for zigbee2mqtt communication.

use crate::config::MqttConfig;
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Message received from MQTT broker.
#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}

/// MQTT client for zigbee2mqtt communication.
pub struct MqttClient {
    client: AsyncClient,
    event_loop: EventLoop,
}

impl MqttClient {
    /// Create a new MQTT client from configuration.
    pub fn new(config: &MqttConfig) -> Self {
        let mut options =
            MqttOptions::new(&config.client_id, &config.broker_host, config.broker_port);
        options.set_keep_alive(Duration::from_secs(30));

        // Set credentials if provided
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            options.set_credentials(username, password);
        }

        let (client, event_loop) = AsyncClient::new(options, 100);

        Self { client, event_loop }
    }

    /// Subscribe to a topic.
    pub async fn subscribe(&self, topic: &str) -> Result<(), rumqttc::ClientError> {
        info!("Subscribing to MQTT topic: {}", topic);
        self.client.subscribe(topic, QoS::AtMostOnce).await
    }

    /// Publish a message to a topic.
    pub async fn publish(&self, topic: &str, payload: &str) -> Result<(), rumqttc::ClientError> {
        debug!("Publishing to {}: {}", topic, payload);
        self.client
            .publish(topic, QoS::AtMostOnce, false, payload.as_bytes())
            .await
    }

    /// Run the MQTT event loop and forward messages to the provided channel.
    ///
    /// Optionally signals when connected via the `connected_tx` oneshot.
    /// This method runs indefinitely, processing MQTT events and sending
    /// received messages through the channel.
    pub async fn run(
        mut self,
        tx: mpsc::Sender<MqttMessage>,
        connected_tx: Option<oneshot::Sender<()>>,
    ) {
        info!("Starting MQTT event loop");
        let mut connected_tx = connected_tx;

        loop {
            match self.event_loop.poll().await {
                Ok(event) => {
                    match &event {
                        Event::Incoming(Packet::ConnAck(_)) => {
                            info!("[MQTT] Connected to broker");
                            // Signal that we're connected
                            if let Some(tx) = connected_tx.take() {
                                let _ = tx.send(());
                            }
                        }
                        Event::Incoming(Packet::Publish(publish)) => {
                            let topic = publish.topic.clone();
                            let payload = match String::from_utf8(publish.payload.to_vec()) {
                                Ok(s) => s,
                                Err(e) => {
                                    warn!("Invalid UTF-8 in MQTT payload: {}", e);
                                    continue;
                                }
                            };

                            debug!("Received MQTT message on {}: {}", topic, payload);

                            let msg = MqttMessage { topic, payload };
                            if tx.send(msg).await.is_err() {
                                error!("MQTT message channel closed");
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("MQTT connection error: {:?}", e);
                    // Wait before reconnecting
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Get a clone of the async client for publishing from other tasks.
    pub fn client(&self) -> AsyncClient {
        self.client.clone()
    }
}
