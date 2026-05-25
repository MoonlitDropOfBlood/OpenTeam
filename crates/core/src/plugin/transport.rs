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
        let resp = send_request(&req).await.unwrap();
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
