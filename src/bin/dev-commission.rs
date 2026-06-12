//! Development tool for commissioning the virtual matter bridge.
//!
//! This tool connects to python-matter-server via WebSocket and sends
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

/// Default python-matter-server WebSocket URL
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
    /// Commission the virtual matter bridge to python-matter-server
    Commission {
        /// Override the discriminator
        #[arg(long, env = "MATTER_DISCRIMINATOR", default_value_t = DEFAULT_DISCRIMINATOR)]
        discriminator: u16,

        /// Override the passcode
        #[arg(long, env = "MATTER_PASSCODE", default_value_t = DEFAULT_PASSCODE)]
        passcode: u32,
    },
    /// Remove a commissioned node from python-matter-server
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

    println!("Connecting to python-matter-server at {}...", cli.server);

    let (ws_stream, _) = connect_async(&cli.server).await.map_err(|e| {
        eprintln!("Failed to connect to {}", cli.server);
        eprintln!("Make sure python-matter-server is running and accessible.");
        eprintln!("Error: {}", e);
        e
    })?;

    println!("Connected!");

    let (mut write, mut read) = ws_stream.split();

    match cli.command {
        Commands::Commission {
            discriminator,
            passcode,
        } => {
            let pairing_code = generate_pairing_code(discriminator, passcode);
            println!("Commissioning with code: {}", pairing_code);

            send_ws_request(
                &mut write,
                "1",
                "commission_with_code",
                Some(serde_json::json!({
                    "code": pairing_code
                })),
            )
            .await?;

            // Wait for response (with timeout)
            println!("Waiting for commissioning response (this may take 30-60 seconds)...");
            let timeout = tokio::time::timeout(
                Duration::from_secs(120),
                wait_for_ws_response(&mut read, "1", |text| println!("Received: {}", text)),
            )
            .await;

            match timeout {
                Ok(Ok(Some(response))) => {
                    if let Some(error_code) = response.error_code {
                        eprintln!("Commissioning failed with error code: {}", error_code);
                        if let Some(details) = response.details {
                            eprintln!("Details: {}", details);
                        }
                    } else if let Some(result) = response.result {
                        println!("Commissioning successful!");
                        println!("Result: {}", serde_json::to_string_pretty(&result)?);
                    }
                }
                Ok(Ok(None)) => {
                    eprintln!("Connection closed before receiving response");
                }
                Ok(Err(e)) => {
                    eprintln!("WebSocket error: {}", e);
                }
                Err(_) => {
                    eprintln!("Timeout waiting for commissioning response");
                    eprintln!("The device may still be commissioning in the background.");
                    eprintln!("Check Home Assistant for the new device.");
                }
            }
        }
        Commands::Remove { node_id } => {
            println!("Removing node {}...", node_id);

            send_ws_request(
                &mut write,
                "1",
                "remove_node",
                Some(serde_json::json!({
                    "node_id": node_id
                })),
            )
            .await?;

            // Wait for response
            match wait_for_ws_response(&mut read, "1", |text| println!("Received: {}", text)).await
            {
                Ok(Some(response)) => {
                    if response.error_code.is_some() {
                        eprintln!("Remove failed: {:?}", response.details);
                    } else {
                        println!("Node {} removed successfully", node_id);
                    }
                }
                Ok(None) => {
                    eprintln!("Connection closed before receiving response");
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                }
            }
        }
        Commands::Status => {
            println!("Getting node status...");

            send_ws_request(&mut write, "1", "get_nodes", None).await?;

            // Wait for response
            match wait_for_ws_response(&mut read, "1", |_| {}).await {
                Ok(Some(response)) => {
                    if let Some(result) = response.result {
                        println!("Nodes:\n{}", serde_json::to_string_pretty(&result)?);
                    } else if response.error_code.is_some() {
                        eprintln!("Failed to get nodes: {:?}", response.details);
                    }
                }
                Ok(None) => {
                    eprintln!("Connection closed before receiving response");
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                }
            }
        }
    }

    Ok(())
}
