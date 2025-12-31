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
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

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
        #[arg(long, default_value_t = DEFAULT_DISCRIMINATOR)]
        discriminator: u16,

        /// Override the passcode
        #[arg(long, default_value_t = DEFAULT_PASSCODE)]
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

/// Request message for python-matter-server WebSocket API
#[derive(Debug, Serialize)]
struct WsRequest {
    message_id: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
}

/// Response message from python-matter-server
#[derive(Debug, Deserialize)]
struct WsResponse {
    message_id: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error_code: Option<i32>,
    #[serde(default)]
    details: Option<String>,
}

/// Generate the manual pairing code from discriminator and passcode.
///
/// This matches the format used by Matter for commission_with_code.
fn generate_pairing_code(discriminator: u16, passcode: u32) -> String {
    // Manual pairing code format: VVVVVVVVVVC
    // V = Vendor-specific data (derived from passcode and discriminator)
    // C = Check digit
    //
    // The actual algorithm is complex. For development, we use the known
    // test code that rs-matter generates for discriminator 3840, passcode 20202021.
    //
    // If you need other codes, run the bridge and copy the manual code from output.
    if discriminator == DEFAULT_DISCRIMINATOR && passcode == DEFAULT_PASSCODE {
        "35325335079".to_string()
    } else {
        // For custom values, the user should read the code from bridge output
        panic!(
            "Custom discriminator/passcode requires manual code extraction from bridge output.\n\
            Run the bridge and copy the manual pairing code shown."
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

            let request = WsRequest {
                message_id: "1".to_string(),
                command: "commission_with_code".to_string(),
                args: Some(serde_json::json!({
                    "code": pairing_code
                })),
            };

            let msg = serde_json::to_string(&request)?;
            println!("Sending: {}", msg);
            write.send(Message::Text(msg.into())).await?;

            // Wait for response (with timeout)
            println!("Waiting for commissioning response (this may take 30-60 seconds)...");
            let timeout = tokio::time::timeout(Duration::from_secs(120), async {
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let text_str: &str = &text;
                            if let Ok(response) = serde_json::from_str::<WsResponse>(text_str)
                                && response.message_id == "1"
                            {
                                return Some(response);
                            }
                            // Print other messages for debugging
                            println!("Received: {}", text);
                        }
                        Ok(Message::Close(_)) => {
                            println!("Server closed connection");
                            return None;
                        }
                        Err(e) => {
                            eprintln!("WebSocket error: {}", e);
                            return None;
                        }
                        _ => {}
                    }
                }
                None
            })
            .await;

            match timeout {
                Ok(Some(response)) => {
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
                Ok(None) => {
                    eprintln!("Connection closed before receiving response");
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

            let request = WsRequest {
                message_id: "1".to_string(),
                command: "remove_node".to_string(),
                args: Some(serde_json::json!({
                    "node_id": node_id
                })),
            };

            let msg = serde_json::to_string(&request)?;
            write.send(Message::Text(msg.into())).await?;

            // Wait for response
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    let text_str: &str = &text;
                    if let Ok(response) = serde_json::from_str::<WsResponse>(text_str)
                        && response.message_id == "1"
                    {
                        if response.error_code.is_some() {
                            eprintln!("Remove failed: {:?}", response.details);
                        } else {
                            println!("Node {} removed successfully", node_id);
                        }
                        break;
                    }
                    println!("Received: {}", text);
                }
            }
        }
        Commands::Status => {
            println!("Getting node status...");

            let request = WsRequest {
                message_id: "1".to_string(),
                command: "get_nodes".to_string(),
                args: None,
            };

            let msg = serde_json::to_string(&request)?;
            write.send(Message::Text(msg.into())).await?;

            // Wait for response
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    let text_str: &str = &text;
                    if let Ok(response) = serde_json::from_str::<WsResponse>(text_str)
                        && response.message_id == "1"
                    {
                        if let Some(result) = response.result {
                            println!("Nodes:\n{}", serde_json::to_string_pretty(&result)?);
                        } else if response.error_code.is_some() {
                            eprintln!("Failed to get nodes: {:?}", response.details);
                        }
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
