use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Manages the Node.js plugin host subprocess
pub struct PluginHost {
    child: Arc<Mutex<Option<Child>>>,
    #[allow(dead_code)]
    request_id: Arc<Mutex<u64>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            request_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Start the Node.js plugin host process
    pub async fn start(&self, host_path: &str) -> Result<(), String> {
        let child = Command::new("node")
            .arg(host_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Spawn plugin host: {e}"))?;

        tracing::info!("Plugin host started (pid: {:?})", child.id());
        let mut guard = self.child.lock().await;
        *guard = Some(child);
        Ok(())
    }

    /// Stop the plugin host process
    pub async fn stop(&self) -> Result<(), String> {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            child.kill().await.map_err(|e| format!("Kill host: {e}"))?;
            child.wait().await.map_err(|e| format!("Wait host: {e}"))?;
            tracing::info!("Plugin host stopped");
        }
        Ok(())
    }
}

/// Send a JSON-RPC request to the Node.js host and await response
/// Real IPC: writes JSON-RPC request to child stdin, reads response from child stdout
pub async fn send_request_to_host(
    request: &JsonRpcRequest,
    child_opt: &Arc<Mutex<Option<Child>>>,
) -> Result<JsonRpcResponse, String> {
    let request_str = serde_json::to_string(request)
        .map_err(|e| format!("Serialize: {e}"))?;

    let mut guard = child_opt.lock().await;
    let child = guard.as_mut().ok_or("Plugin host not running")?;

    // Write request to stdin
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(request_str.as_bytes()).await
            .map_err(|e| format!("Write stdin: {e}"))?;
        stdin.write_all(b"\n").await.ok();
    }

    // Read response from stdout
    let stdout = child.stdout.as_mut()
        .ok_or("No stdout from plugin host")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        reader.read_line(&mut line),
    )
    .await
    .map_err(|_| "Plugin host timed out".to_string())?
    .map_err(|e| format!("Read stdout: {e}"))?;

    serde_json::from_str(&line)
        .map_err(|e| format!("Parse response: {e}\nRaw: {line}"))
}

/// Send a JSON-RPC request via stdin/stdout to the Node.js host (stub)
/// Phase 3 V2: actual IPC with PluginHost — use send_request_to_host for real communication
pub async fn send_request_stub(request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
    let input = serde_json::to_string(request)
        .map_err(|e| format!("Serialize request: {e}"))?;
    tracing::debug!("Plugin IPC request (stub): {input}");
    Ok(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id: request.id,
        result: Some(serde_json::json!({"status": "stub"})),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_json_rpc_roundtrip() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: 42,
            method: "ping".into(),
            params: serde_json::json!({}),
        };

        // Verify serialization roundtrip
        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.method, "ping");

        // Verify stub response
        let resp = send_request_stub(&req).await.unwrap();
        assert_eq!(resp.id, 42);
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_json_rpc_error_serde() {
        let err = JsonRpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32601"));
        assert!(json.contains("Method not found"));
    }
}
