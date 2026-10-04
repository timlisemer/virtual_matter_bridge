//! Development tool for commissioning the virtual matter bridge.
//!
//! This tool connects to Matter Server via WebSocket and sends
//! commissioning commands, eliminating the need for phone-based QR scanning.
//!
//! Usage:
//!   cargo run --bin dev-commission -- commission
//!   cargo run --bin dev-commission -- remove <node-id>
//!   cargo run --bin dev-commission -- status

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use virtual_matter_bridge::commissioning::{
    generate_pairing_code, send_ws_request, wait_for_ws_response,
};

/// Default Matter Server WebSocket URL
const DEFAULT_MATTER_SERVER_URL: &str = "ws://localhost:5580/ws";

/// Default discriminator (from rs-matter TEST_DEV_COMM)
const DEFAULT_DISCRIMINATOR: u16 = 3840;

/// Default passcode (from rs-matter TEST_DEV_COMM)
const DEFAULT_PASSCODE: u32 = 20202021;

#[derive(Parser)]
#[command(name = "dev-commission")]
#[command(about = "Development tool for commissioning virtual matter bridge")]
struct Cli {
    /// Matter server WebSocket URL
    #[arg(long, env = "MATTER_SERVER_URL", default_value = DEFAULT_MATTER_SERVER_URL)]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Commission the virtual matter bridge to Matter Server
    Commission {
        /// Override the discriminator
        #[arg(long, env = "MATTER_DISCRIMINATOR", default_value_t = DEFAULT_DISCRIMINATOR)]
        discriminator: u16,

        /// Override the passcode
        #[arg(long, env = "MATTER_PASSCODE", default_value_t = DEFAULT_PASSCODE)]
        passcode: u32,
    },
    /// Remove a commissioned node from Matter Server
    Remove {
        /// Node ID to remove
        node_id: u64,
    },
    /// Get status of all commissioned nodes
    Status,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load .env file before parsing CLI args (clap reads env vars during parse)
    virtual_matter_bridge::config::load_dotenv();

    let cli = Cli::parse();

    println!("Connecting to Matter Server at {}...", cli.server);

    let (ws_stream, _) = connect_async(&cli.server).await.map_err(|e| {
        eprintln!("Failed to connect to {}", cli.server);
        eprintln!("Make sure Matter Server is running and accessible.");
        eprintln!("Error: {}", e);
        e
    })?;

    println!("Connected!");

    let (mut write, mut read) = ws_stream.split();

    let (command, args, timeout_seconds) = match &cli.command {
        Commands::Commission {
            discriminator,
            passcode,
        } => {
            let pairing_code = generate_pairing_code(*discriminator, *passcode);
            println!("Commissioning with code: {}", pairing_code);
            (
                "commission_with_code",
                Some(serde_json::json!({
                    "code": pairing_code,
                    "network_only": true
                })),
                120,
            )
        }
        Commands::Remove { node_id } => {
            println!("Removing node {}...", node_id);
            (
                "remove_node",
                Some(serde_json::json!({"node_id": node_id})),
                30,
            )
        }
        Commands::Status => {
            println!("Getting node status...");
            ("get_nodes", None, 30)
        }
    };

    send_ws_request(&mut write, "1", command, args).await?;
    let response = tokio::time::timeout(
        Duration::from_secs(timeout_seconds),
        wait_for_ws_response(&mut read, "1", |_| {}),
    )
    .await
    .map_err(|_| format!("Timeout waiting for {} response", command))??
    .ok_or("Connection closed before receiving response")?;

    if let Some(error_code) = response.error_code {
        return Err(format!(
            "{} failed (error {}): {}",
            command,
            error_code,
            response.details.unwrap_or_default()
        )
        .into());
    }

    match cli.command {
        Commands::Commission { .. } => {
            let result = response
                .result
                .ok_or("No commissioning result in response")?;
            println!("Commissioning successful!");
            println!("Result: {}", serde_json::to_string_pretty(&result)?);
        }
        Commands::Remove { node_id } => {
            println!("Node {} removed successfully", node_id);
        }
        Commands::Status => {
            let result = response.result.ok_or("No nodes in response")?;
            if !result.is_array() {
                return Err("Nodes is not an array".into());
            }
            println!("Nodes:\n{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}
