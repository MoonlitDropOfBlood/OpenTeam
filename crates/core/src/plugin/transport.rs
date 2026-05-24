use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Send a JSON-RPC request via stdin/stdout to the Node.js host (Phase 3 V1: stub)
pub async fn send_request(request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
    let input = serde_json::to_string(request)
        .map_err(|e| format!("Serialize request: {e}"))?;
    tracing::debug!("Plugin IPC request: {input}");
    // Phase 3 V1: returns stub response
    Ok(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id: request.id,
        result: Some(serde_json::json!({"status": "stub"})),
        error: None,
    })
}
