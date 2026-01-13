mod dev_att;
mod device_info;
mod logging_udp;
mod netif;
mod stack;

pub mod clusters;
pub mod device_types;
pub mod endpoints;
pub mod events;
pub mod handler_bridge;
pub mod virtual_device;

pub use stack::run_matter_stack;

// Re-export from endpoints for convenience
pub use endpoints::controls;
pub use endpoints::sensors;

// Re-export virtual device types
pub use virtual_device::{EndpointConfig, VirtualDevice};
