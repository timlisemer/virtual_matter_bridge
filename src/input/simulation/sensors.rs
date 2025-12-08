//! Sensor simulation for testing.
//!
//! Provides simulated sensor state changes for development and testing purposes.

use crate::matter::sensors::{ContactSensor, OccupancySensor};
use log::info;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};

/// Spawn a task that periodically toggles sensor states for testing.
///
/// This simulation toggles both contact and occupancy sensors every 30 seconds.
/// Useful for development and testing Matter subscriptions.
///
/// # Returns
///
/// A `JoinHandle` that can be used to abort the simulation task.
pub fn run_sensor_simulation(
    contact_sensor: Arc<ContactSensor>,
    occupancy_sensor: Arc<OccupancySensor>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let new_contact = contact_sensor.toggle();
            info!("[Sim] Contact sensor toggled to: {}", new_contact);
            let new_occupancy = occupancy_sensor.toggle();
            info!("[Sim] Occupancy sensor toggled to: {}", new_occupancy);
        }
    })
}
