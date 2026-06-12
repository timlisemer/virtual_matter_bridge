//! Auto-commissioning to python-matter-server.
//!
//! When `MATTER_SERVER_URL` is set, the bridge automatically commissions
//! itself to python-matter-server on startup.
//!
//! Uses `commission_with_code` with `network_only: true` to commission via
//! mDNS/IP without requiring Bluetooth.

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Request message for python-matter-server WebSocket API.
#[derive(Debug, Serialize)]
pub struct WsRequest {
    message_id: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
}

/// Response message from python-matter-server.
#[derive(Debug, Deserialize)]
pub struct WsResponse {
    pub message_id: String,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error_code: Option<i32>,
    #[serde(default)]
    pub details: Option<String>,
}

impl WsRequest {
    pub fn new(
        message_id: impl Into<String>,
        command: impl Into<String>,
        args: Option<serde_json::Value>,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            command: command.into(),
            args,
        }
    }
}

pub async fn send_ws_request<W>(
    write: &mut W,
    message_id: impl Into<String>,
    command: impl Into<String>,
    args: Option<serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    W: Sink<Message> + Unpin,
    W::Error: std::error::Error + Send + Sync + 'static,
{
    let request = WsRequest::new(message_id, command, args);
    write
        .send(Message::Text(serde_json::to_string(&request)?.into()))
        .await?;
    Ok(())
}

pub async fn wait_for_ws_response<R>(
    read: &mut R,
    message_id: &str,
    mut on_other_text: impl FnMut(&str),
) -> Result<Option<WsResponse>, tokio_tungstenite::tungstenite::Error>
where
    R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let text_str: &str = &text;
                if let Ok(response) = serde_json::from_str::<WsResponse>(text_str)
                    && response.message_id == message_id
                {
                    return Ok(Some(response));
                }
                on_other_text(text_str);
            }
            Ok(Message::Close(_)) => return Ok(None),
            Err(e) => return Err(e),
            _ => {}
        }
    }
    Ok(None)
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

/// Remove existing bridge nodes from python-matter-server.
///
/// This should be called before re-commissioning after a schema change
/// to clean up orphaned device entries from the controller.
///
/// Returns the number of nodes removed.
pub async fn remove_bridge_nodes(
    server_url: &str,
    vendor_id: u16,
) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "[Commission] Connecting to {} to cleanup old nodes",
        server_url
    );

    let (ws_stream, _) = connect_async(server_url)
        .await
        .map_err(|e| format!("Failed to connect to {}: {}", server_url, e))?;

    let (mut write, mut read) = ws_stream.split();

    // Get list of all nodes
    send_ws_request(&mut write, "get-nodes", "get_nodes", None).await?;

    // Wait for response
    let nodes_response = tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_ws_response(&mut read, "get-nodes", |_| {}),
    )
    .await
    .map_err(|_| "Timeout waiting for get_nodes response")??
    .ok_or("Connection closed")?;

    // Parse nodes and find ones matching our vendor ID
    let nodes = nodes_response.result.ok_or("No nodes in response")?;
    let nodes_array = nodes.as_array().ok_or("Nodes is not an array")?;

    let mut removed_count = 0u32;
    for node in nodes_array {
        // Extract node_id and vendor_id from the node data
        let node_id = node.get("node_id").and_then(|v| v.as_u64());
        let node_vendor_id = node
            .get("attributes")
            .and_then(|a| a.get("0"))
            .and_then(|ep| ep.get("40"))
            .and_then(|basic| basic.get("1"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u16);

        if let (Some(nid), Some(vid)) = (node_id, node_vendor_id)
            && vid == vendor_id
        {
            info!(
                "[Commission] Removing old bridge node {} (vendor_id={:#06x})",
                nid, vid
            );

            let remove_message_id = format!("remove-{}", nid);
            if let Err(e) = send_ws_request(
                &mut write,
                remove_message_id.as_str(),
                "remove_node",
                Some(serde_json::json!({ "node_id": nid })),
            )
            .await
            {
                warn!(
                    "[Commission] Failed to send remove request for node {}: {}",
                    nid, e
                );
                continue;
            }

            // Wait for remove response
            let remove_timeout = tokio::time::timeout(
                Duration::from_secs(30),
                wait_for_ws_response(&mut read, &remove_message_id, |_| {}),
            )
            .await;

            match remove_timeout {
                Ok(Ok(Some(response))) if response.error_code.is_none() => {
                    info!("[Commission] Successfully removed node {}", nid);
                    removed_count += 1;
                }
                Ok(Ok(Some(response))) => {
                    warn!(
                        "[Commission] Failed to remove node {}: {:?}",
                        nid, response.details
                    );
                }
                Ok(Ok(None)) => {
                    warn!("[Commission] Connection closed while removing node {}", nid);
                }
                Ok(Err(e)) => {
                    warn!("[Commission] WebSocket error removing node {}: {}", nid, e);
                }
                Err(_) => {
                    warn!("[Commission] Timeout removing node {}", nid);
                }
            }
        }
    }

    if removed_count > 0 {
        info!(
            "[Commission] Removed {} old bridge node(s) from controller",
            removed_count
        );
    } else {
        info!("[Commission] No old bridge nodes found to remove");
    }

    Ok(removed_count)
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
    send_ws_request(
        &mut write,
        "auto-commission",
        "commission_with_code",
        Some(serde_json::json!({
            "code": pairing_code,
            "network_only": true
        })),
    )
    .await?;

    // Wait for response (with timeout)
    info!("[Commission] Waiting for commissioning response...");
    let timeout = tokio::time::timeout(
        Duration::from_secs(120),
        wait_for_ws_response(&mut read, "auto-commission", |_| {}),
    )
    .await;

    match timeout {
        Ok(Ok(Some(response))) => {
            if let Some(error_code) = response.error_code {
                let details = response.details.unwrap_or_default();
                Err(format!("Commissioning failed (error {}): {}", error_code, details).into())
            } else {
                info!("[Commission] Successfully commissioned to python-matter-server!");
                Ok(())
            }
        }
        Ok(Ok(None)) => Err("Connection closed before receiving response".into()),
        Ok(Err(e)) => {
            warn!("[Commission] WebSocket error: {}", e);
            Err(format!("WebSocket error before receiving response: {}", e).into())
        }
        Err(_) => {
            // Timeout isn't necessarily an error - commissioning may still succeed
            warn!("[Commission] Timeout waiting for response. Device may still be commissioning.");
            Ok(())
        }
    }
}
