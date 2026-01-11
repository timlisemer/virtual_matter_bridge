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
/// Matter short manual pairing code is an 11-digit decimal number:
/// - Chunk 1 (1 digit): discriminator bits 11-10
/// - Chunk 2 (5 digits): discriminator bits 9-8 (upper 2 bits) + passcode bits 13-0 (lower 14 bits)
/// - Chunk 3 (4 digits): passcode bits 26-14
/// - Check digit (1 digit): Verhoeff checksum
///
/// Reference: Matter spec Section 5.1.4.1, connectedhomeip ManualSetupPayloadGenerator.h
fn generate_pairing_code(discriminator: u16, passcode: u32) -> String {
    // Chunk 1: top 2 bits of discriminator (bits 11-10)
    let chunk1 = (discriminator >> 10) & 0x03;

    // Chunk 2: discriminator bits 9-8 in upper 2 bits, passcode bits 13-0 in lower 14 bits
    let discriminator_bits_9_8 = ((discriminator >> 8) & 0x03) as u32;
    let passcode_bits_13_0 = passcode & 0x3FFF;
    let chunk2 = (discriminator_bits_9_8 << 14) | passcode_bits_13_0;

    // Chunk 3: passcode bits 26-14
    let chunk3 = (passcode >> 14) & 0x1FFF;

    // Format as 10 digits (1 + 5 + 4), then add Verhoeff check digit
    let payload = format!("{}{:05}{:04}", chunk1, chunk2, chunk3);
    let check_digit = verhoeff_checksum(&payload);
    format!("{}{}", payload, check_digit)
}

/// Compute Verhoeff check digit for a string of decimal digits.
fn verhoeff_checksum(input: &str) -> u8 {
    // Verhoeff dihedral group D5 multiplication table
    const D: [[u8; 10]; 10] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [1, 2, 3, 4, 0, 6, 7, 8, 9, 5],
        [2, 3, 4, 0, 1, 7, 8, 9, 5, 6],
        [3, 4, 0, 1, 2, 8, 9, 5, 6, 7],
        [4, 0, 1, 2, 3, 9, 5, 6, 7, 8],
        [5, 9, 8, 7, 6, 0, 4, 3, 2, 1],
        [6, 5, 9, 8, 7, 1, 0, 4, 3, 2],
        [7, 6, 5, 9, 8, 2, 1, 0, 4, 3],
        [8, 7, 6, 5, 9, 3, 2, 1, 0, 4],
        [9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
    ];

    // Verhoeff permutation table
    const P: [[u8; 10]; 8] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [1, 5, 7, 6, 2, 8, 3, 0, 9, 4],
        [5, 8, 0, 3, 7, 9, 6, 1, 4, 2],
        [8, 9, 1, 6, 0, 4, 3, 5, 2, 7],
        [9, 4, 5, 3, 1, 2, 6, 8, 7, 0],
        [4, 2, 8, 6, 5, 7, 3, 9, 0, 1],
        [2, 7, 9, 3, 8, 0, 6, 4, 1, 5],
        [7, 0, 4, 6, 9, 1, 3, 2, 5, 8],
    ];

    // Verhoeff inverse table
    const INV: [u8; 10] = [0, 4, 3, 2, 1, 5, 6, 7, 8, 9];

    let digits: Vec<u8> = input.bytes().map(|b| b - b'0').collect();
    let mut c: u8 = 0;
    for (i, &d) in digits.iter().rev().enumerate() {
        c = D[c as usize][P[(i + 1) % 8][d as usize] as usize];
    }
    INV[c as usize]
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
