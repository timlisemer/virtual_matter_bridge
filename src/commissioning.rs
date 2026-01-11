//! Auto-commissioning to python-matter-server.
//!
//! When `MATTER_SERVER_URL` is set, the bridge automatically commissions
//! itself to python-matter-server on startup.
//!
//! Uses `commission_with_code` with `network_only: true` to commission via
//! mDNS/IP without requiring Bluetooth.

use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Request message for python-matter-server WebSocket API.
#[derive(Debug, Serialize)]
struct WsRequest {
    message_id: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
}

/// Response message from python-matter-server.
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
/// Matter short manual pairing code is an 11-digit decimal number.
/// Reference: Matter spec Section 5.1.4.1
pub fn generate_pairing_code(discriminator: u16, passcode: u32) -> String {
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

    const INV: [u8; 10] = [0, 4, 3, 2, 1, 5, 6, 7, 8, 9];

    let digits: Vec<u8> = input.bytes().map(|b| b - b'0').collect();
    let mut c: u8 = 0;
    for (i, &d) in digits.iter().rev().enumerate() {
        c = D[c as usize][P[(i + 1) % 8][d as usize] as usize];
    }
    INV[c as usize]
}

/// Auto-commission the bridge to python-matter-server.
///
/// This function waits for the bridge to be ready, then connects to
/// python-matter-server and sends a commission request.
pub async fn auto_commission(
    server_url: &str,
    discriminator: u16,
    passcode: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Wait for the Matter stack to be ready
    info!("[Commission] Waiting for Matter stack to initialize...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    info!(
        "[Commission] Connecting to python-matter-server at {}",
        server_url
    );

    let (ws_stream, _) = connect_async(server_url)
        .await
        .map_err(|e| format!("Failed to connect to {}: {}", server_url, e))?;

    info!("[Commission] Connected to python-matter-server");

    let (mut write, mut read) = ws_stream.split();

    // Generate pairing code from discriminator and passcode
    let pairing_code = generate_pairing_code(discriminator, passcode);
    info!(
        "[Commission] Commissioning with code {} (network_only)",
        pairing_code
    );

    // Send commission request using network discovery (no Bluetooth)
    let request = WsRequest {
        message_id: "auto-commission".to_string(),
        command: "commission_with_code".to_string(),
        args: Some(serde_json::json!({
            "code": pairing_code,
            "network_only": true
        })),
    };

    let msg = serde_json::to_string(&request)?;
    write.send(Message::Text(msg.into())).await?;

    // Wait for response (with timeout)
    info!("[Commission] Waiting for commissioning response...");
    let timeout = tokio::time::timeout(Duration::from_secs(120), async {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let text_str: &str = &text;
                    if let Ok(response) = serde_json::from_str::<WsResponse>(text_str)
                        && response.message_id == "auto-commission"
                    {
                        return Some(response);
                    }
                }
                Ok(Message::Close(_)) => {
                    warn!("[Commission] Server closed connection");
                    return None;
                }
                Err(e) => {
                    warn!("[Commission] WebSocket error: {}", e);
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
                let details = response.details.unwrap_or_default();
                Err(format!("Commissioning failed (error {}): {}", error_code, details).into())
            } else {
                info!("[Commission] Successfully commissioned to python-matter-server!");
                Ok(())
            }
        }
        Ok(None) => Err("Connection closed before receiving response".into()),
        Err(_) => {
            // Timeout isn't necessarily an error - commissioning may still succeed
            warn!("[Commission] Timeout waiting for response. Device may still be commissioning.");
            Ok(())
        }
    }
}
