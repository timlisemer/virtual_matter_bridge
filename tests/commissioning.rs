use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use virtual_matter_bridge::commissioning::{generate_pairing_code, remove_bridge_nodes};

async fn cli_response(command: &[&str], response: Value) -> std::process::Output {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        ws.send(Message::Text(
            json!({"schema_version": 13}).to_string().into(),
        ))
        .await
        .unwrap();
        let request = ws.next().await.unwrap().unwrap();
        let request: Value = serde_json::from_str(request.to_text().unwrap()).unwrap();
        assert_eq!(request["message_id"], "1");
        if request["command"] == "commission_with_code" {
            assert_eq!(request["args"]["code"], "34970112332");
            assert_eq!(request["args"]["network_only"], true);
        }
        ws.send(Message::Text(
            json!({"event": "node_updated", "data": {}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        ws.send(Message::Text(response.to_string().into()))
            .await
            .unwrap();
        ws.close(None).await.unwrap();
    });
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_dev-commission"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args(["--server", &url])
        .args(command)
        .output()
        .await
        .unwrap();
    server.await.unwrap();
    output
}

#[test]
fn default_manual_pairing_code_matches_the_sdk_example() {
    assert_eq!(generate_pairing_code(3840, 20202021), "34970112332");
}

#[tokio::test]
async fn cli_status_accepts_server_info_and_events_before_the_response() {
    let output = cli_response(&["status"], json!({"message_id":"1", "result":[]})).await;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Nodes:"));
}

#[tokio::test]
async fn cli_remove_accepts_a_null_success_result() {
    let output = cli_response(&["remove", "123"], json!({"message_id":"1", "result":null})).await;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("removed successfully"));
}

#[tokio::test]
async fn cli_server_errors_return_a_failed_exit_status() {
    for command in [vec!["status"], vec!["remove", "123"], vec!["commission"]] {
        let output = cli_response(
            &command,
            json!({"message_id":"1", "error_code":5, "details":"Test server error"}),
        )
        .await;
        assert!(
            !output.status.success(),
            "{command:?} returned success after a server error"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("Test server error"));
    }
}

#[tokio::test]
async fn cli_connection_close_returns_a_failed_exit_status() {
    let output = cli_response(&["status"], json!({"event":"server_shutdown"})).await;
    assert!(!output.status.success());
}

#[tokio::test]
async fn cli_commission_accepts_a_node_result() {
    let output = cli_response(
        &["commission"],
        json!({"message_id":"1", "result":{"node_id":123}}),
    )
    .await;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Commissioning successful!"));
}

#[tokio::test]
async fn cli_status_rejects_a_missing_or_invalid_node_list() {
    for result in [Value::Null, json!({})] {
        let output = cli_response(&["status"], json!({"message_id":"1", "result":result})).await;
        assert!(!output.status.success());
    }
}

#[tokio::test]
async fn cleanup_uses_flat_vendor_id_attributes_and_keeps_other_vendors() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        let request = ws.next().await.unwrap().unwrap();
        let request: Value = serde_json::from_str(request.to_text().unwrap()).unwrap();
        assert_eq!(request["command"], "get_nodes");
        ws.send(Message::Text(
            json!({"message_id":"get-nodes", "result":[
                {"node_id":123, "attributes":{"0/40/1":"Bridge", "0/40/2":65521}},
            {"node_id":456, "attributes":{"0/40/1":"Other vendor", "0/40/2":1234}},
            {"node_id":789, "attributes":{"0/40/2":131057}}
            ]})
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
        let request = ws.next().await.unwrap().unwrap();
        let request: Value = serde_json::from_str(request.to_text().unwrap()).unwrap();
        assert_eq!(request["command"], "remove_node");
        assert_eq!(request["args"]["node_id"], 123);
        ws.send(Message::Text(
            json!({"message_id":"remove-123", "result":null})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    });
    let count = remove_bridge_nodes(&url, 0xFFF1).await.unwrap();
    assert_eq!(count, 1);
    server.await.unwrap();
}
