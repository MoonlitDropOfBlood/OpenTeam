use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex as StdMutex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify, broadcast, mpsc, oneshot};

use super::types::*;
use crate::CoreError;

/// Timeout for JSON-RPC calls to the Node.js plugin process.
const IPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

// ---- Message types for the internal IPC loop ----

enum IpcCommand {
    Call {
        method: String,
        params: serde_json::Value,
        tx: oneshot::Sender<Result<serde_json::Value, CoreError>>,
    },
    Shutdown,
}

// ---- FeishuChannelBridge ----

/// Manages the Node.js Channel SDK plugin subprocess via JSON-RPC over stdio.
///
/// # Lifecycle
/// 1. `new()` — allocate bridge resources
/// 2. `start(app_id, app_secret)` — spawn Node.js plugin, establish WebSocket to Feishu
/// 3. Use `send_message()`, `reply_to_message()` etc. during runtime
/// 4. `shutdown()` — disconnect plugin and stop the subprocess
///
/// Incoming Feishu messages are received via `subscribe_messages()`.
#[derive(Clone)]
pub struct FeishuChannelBridge {
    /// Sender for IPC commands to the background I/O task.
    cmd_tx: mpsc::UnboundedSender<IpcCommand>,
    /// Broadcast channel for incoming NormalizedMessages from Feishu.
    msg_tx: broadcast::Sender<NormalizedMessage>,
    /// Watch channel for connection status changes.
    status_tx: tokio::sync::watch::Sender<ChannelStatus>,
    status_rx: tokio::sync::watch::Receiver<ChannelStatus>,
    /// Notify on ready (connect complete).
    ready_notify: Arc<Notify>,
    /// The Node.js child process handle.
    child: Arc<Mutex<Option<Child>>>,
    /// Track whether start() has been called successfully.
    started: Arc<Mutex<bool>>,
}

impl FeishuChannelBridge {
    /// Create a new bridge (does NOT spawn the plugin yet — call `start()`).
    pub fn new() -> Self {
        let (msg_tx, _) = broadcast::channel(256);
        let (status_tx, status_rx) = tokio::sync::watch::channel(ChannelStatus::Disconnected);
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();

        Self {
            cmd_tx,
            msg_tx,
            status_tx,
            status_rx,
            ready_notify: Arc::new(Notify::new()),
            child: Arc::new(Mutex::new(None)),
            started: Arc::new(Mutex::new(false)),
        }
    }

    // ---- Public API ----

    /// Spawn the Node.js plugin process and establish the Feishu Channel connection.
    ///
    /// Blocks until the WebSocket handshake completes or `CONNECT_TIMEOUT` elapses.
    pub async fn start(&mut self, app_id: &str, app_secret: &str) -> Result<(), CoreError> {
        let plugin_path = Self::resolve_plugin_path()?;

// Spawn Node.js process
    let mut child = Command::new("node")
        .arg(&plugin_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
            .map_err(|e| CoreError::Feishu(format!("Failed to spawn feishu-channel plugin: {e}")))?;

        tracing::info!("[ChannelBridge] Plugin spawned (pid: {:?})", child.id());

        let stdin = child.stdin.take()
            .ok_or_else(|| CoreError::Feishu("Plugin has no stdin".into()))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| CoreError::Feishu("Plugin has no stdout".into()))?;

        // Start IPC channels
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        self.cmd_tx = cmd_tx;

        // Pending requests map (shared between write task and read task)
        let pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, CoreError>>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicU64::new(1));

        // Write task: reads from cmd_rx, writes JSON-RPC to stdin
        let write_stdin = Arc::new(tokio::sync::Mutex::new(stdin));
        let w_pending = pending.clone();
        let w_next_id = next_id.clone();
        let w_stdin = write_stdin.clone();
        tokio::spawn(async move {
            Self::write_loop(cmd_rx, w_stdin, w_pending, w_next_id).await;
        });

        // Read task: reads JSON-RPC from stdout, routes to pending or broadcast
        let r_pending = pending.clone();
        let r_msg_tx = self.msg_tx.clone();
        let r_status_tx = self.status_tx.clone();
        let r_ready = self.ready_notify.clone();
        tokio::spawn(async move {
            Self::read_loop(stdout, r_pending, r_msg_tx, r_status_tx, r_ready).await;
        });

        // Stderr reader task: forward plugin logs to tracing (not terminal)
        let stderr = child.stderr.take()
            .ok_or_else(|| CoreError::Feishu("Plugin has no stderr".into()))?;
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => tracing::info!("[plugin] {}", line.trim()),
                    Err(_) => break,
                }
            }
        });

        // Store child handle (stderr already taken above)
        {
            let mut guard = self.child.lock().await;
            *guard = Some(child);
        }

        // Send connect RPC
        let result = self.rpc_call("connect", serde_json::json!({
            "appId": app_id,
            "appSecret": app_secret,
        })).await;

        match result {
            Ok(val) => {
                let bot_name = val["botName"].as_str().unwrap_or("unknown").to_string();
                tracing::info!("[ChannelBridge] Connected as: {bot_name}");
                let mut guard = self.started.lock().await;
                *guard = true;
                Ok(())
            }
            Err(e) => {
                tracing::error!("[ChannelBridge] Connect failed: {e}");
                // Kill the plugin process
                let mut guard = self.child.lock().await;
                if let Some(mut c) = guard.take() {
                    let _ = c.kill().await;
                    let _ = c.wait().await;
                }
                Err(e)
            }
        }
    }

    /// Send a message via the Channel SDK.
    pub async fn send_message(&self, msg: &OutgoingMessage) -> Result<MessageId, CoreError> {
        let content = if msg.mentions.is_empty() {
            serde_json::json!({"markdown": msg.text})
        } else {
            // Prepend @mentions as text
            let _mentions: Vec<MentionInfo> = msg.mentions.iter().map(|m| MentionInfo {
                user_id: m.user_id.clone(),
                name: m.name.clone(),
                is_bot: false,
            }).collect();
            // Prepend @mentions as text since the SDK handles structured mentions via options
            let text = msg.mentions.iter()
                .map(|m| format!("<at user_id=\"{}\">{}</at>", m.user_id, m.name))
                .chain(std::iter::once(msg.text.clone()))
                .collect::<Vec<_>>()
                .join(" ");
            serde_json::json!({"markdown": text})
        };

        let result = self.rpc_call("send", serde_json::json!({
            "chatId": msg.chat_id,
            "content": content,
            "options": {
                "replyInThread": msg.thread_id.is_some(),
            },
        })).await?;

        let message_id = result["messageId"].as_str()
            .ok_or_else(|| CoreError::Feishu("send_message: missing messageId in response".into()))?
            .to_string();
        Ok(message_id)
    }

    /// Reply to a message (in-thread or direct reply).
    pub async fn reply_to_message(
        &self,
        message_id: &str,
        text: &str,
        reply_in_thread: bool,
    ) -> Result<MessageId, CoreError> {
        let result = self.rpc_call("send", serde_json::json!({
            "chatId": "",  // will be resolved by SDK from reply context
            "content": {"markdown": text},
            "options": {
                "replyTo": message_id,
                "replyInThread": reply_in_thread,
            },
        })).await?;

        let msg_id = result["messageId"].as_str()
            .ok_or_else(|| CoreError::Feishu("reply_to_message: missing messageId".into()))?
            .to_string();
        Ok(msg_id)
    }

    /// Send content as a streaming (animated) Feishu card.
    pub async fn stream_reply(
        &self,
        chat_id: &str,
        chunks: Vec<String>,
        reply_to: Option<&str>,
    ) -> Result<(), CoreError> {
        let mut options = serde_json::json!({});
        if let Some(rid) = reply_to {
            options["replyTo"] = serde_json::json!(rid);
        }

        self.rpc_call("stream", serde_json::json!({
            "chatId": chat_id,
            "chunks": chunks,
            "options": options,
        })).await?;
        Ok(())
    }

    /// Check connection status.
    pub async fn check_connection(&self) -> Result<bool, CoreError> {
        if !*self.started.lock().await {
            return Ok(false);
        }
        // Try ping — if plugin is alive, it responds
        match self.rpc_call("ping", serde_json::json!({})).await {
            Ok(_) => {
                let status = self.status_rx.borrow().clone();
                Ok(matches!(status, ChannelStatus::Connected { .. }))
            }
            Err(_) => Ok(false),
        }
    }

    /// Get current connection status (fast, no IPC).
    pub fn connection_status(&self) -> ChannelStatus {
        self.status_rx.borrow().clone()
    }

    /// Get a receiver for incoming Feishu messages.
    pub fn subscribe_messages(&self) -> broadcast::Receiver<NormalizedMessage> {
        self.msg_tx.subscribe()
    }

    /// Get a receiver for connection status changes.
    pub fn subscribe_status(&self) -> tokio::sync::watch::Receiver<ChannelStatus> {
        self.status_rx.clone()
    }

    /// Disconnect from Feishu and stop the Node.js plugin.
    pub async fn shutdown(&self) {
        tracing::info!("[ChannelBridge] Shutting down...");

        // Send disconnect RPC (best-effort)
        let _ = self.rpc_call("disconnect", serde_json::json!({})).await;

        // Send shutdown command to I/O task
        let _ = self.cmd_tx.send(IpcCommand::Shutdown);

        // Kill the subprocess
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            tracing::info!("[ChannelBridge] Plugin process stopped");
        }

        let mut started = self.started.lock().await;
        *started = false;
    }

    // ---- Internal helpers ----

    async fn rpc_call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, CoreError> {
        let (tx, rx) = oneshot::channel();

        self.cmd_tx.send(IpcCommand::Call {
            method: method.to_string(),
            params,
            tx,
        }).map_err(|_| CoreError::Feishu("ChannelBridge: IPC send failed (plugin not running)".into()))?;

        tokio::time::timeout(IPC_TIMEOUT, rx)
            .await
            .map_err(|_| CoreError::Feishu(format!("ChannelBridge: RPC '{method}' timed out after {:?}", IPC_TIMEOUT)))?
            .map_err(|_| CoreError::Feishu(format!("ChannelBridge: RPC '{method}' cancelled (plugin dropped)")))?
    }

    fn resolve_plugin_path() -> Result<String, CoreError> {
        // Check several locations for the plugin script
        let candidates = [
            // Relative to working directory (dev)
            "plugins/feishu-channel/index.js",
            // Relative to executable
            "../plugins/feishu-channel/index.js",
            // Relative to CARGO_MANIFEST_DIR at build time
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/feishu-channel/index.js"),
        ];

        for path in &candidates {
            let p = std::path::Path::new(path);
            if p.exists() {
                return Ok(path.to_string());
            }
        }

        // Last resort: search relative to CARGO_MANIFEST_DIR
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let fallback = format!("{}/../../plugins/feishu-channel/index.js", manifest_dir);
        let fb_path = std::path::Path::new(&fallback);
        if fb_path.exists() {
            return Ok(fallback);
        }

        Err(CoreError::Config(
            "feishu-channel plugin not found at any expected location. \
             Expected at: plugins/feishu-channel/index.js".into()
        ))
    }

    // ---- Background I/O tasks ----

    /// Write loop: receive IPC commands, serialize to JSON-RPC, write to stdin.
    async fn write_loop(
        mut cmd_rx: mpsc::UnboundedReceiver<IpcCommand>,
        stdin: Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>,
        pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, CoreError>>>>>,
        next_id: Arc<AtomicU64>,
    ) {
        use tokio::io::AsyncWriteExt;

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                IpcCommand::Call { method, params, tx } => {
                    let id = next_id.fetch_add(1, Ordering::SeqCst);

                    // Store the response channel
                    {
                        let mut map = pending.lock().unwrap();
                        map.insert(id, tx);
                    }

                    // Serialize and write
                    let request = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "method": method,
                        "params": params,
                    });
                    let line = serde_json::to_string(&request).unwrap_or_default();

                    let mut guard = stdin.lock().await;
                    if let Err(e) = guard.write_all(line.as_bytes()).await {
                        tracing::error!("[ChannelBridge] Write to stdin failed: {e}");
                        // Notify pending request of failure
                        let mut map = pending.lock().unwrap();
                        if let Some(tx) = map.remove(&id) {
                            let _ = tx.send(Err(CoreError::Feishu(format!("stdin write error: {e}"))));
                        }
                        break;
                    }
                    if let Err(e) = guard.write_all(b"\n").await {
                        tracing::error!("[ChannelBridge] Write newline failed: {e}");
                        break;
                    }
                }
                IpcCommand::Shutdown => {
                    break;
                }
            }
        }
        tracing::debug!("[ChannelBridge] Write loop ended");
    }

    /// Read loop: read JSON-RPC lines from stdout, dispatch to pending or broadcast.
    async fn read_loop(
        stdout: tokio::process::ChildStdout,
        pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, CoreError>>>>>,
        msg_tx: broadcast::Sender<NormalizedMessage>,
        status_tx: tokio::sync::watch::Sender<ChannelStatus>,
        ready_notify: Arc<Notify>,
    ) {
        let mut reader = BufReader::new(stdout);
        let mut line = String::with_capacity(4096);

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    tracing::warn!("[ChannelBridge] Plugin stdout closed");
                    let _ = status_tx.send(ChannelStatus::Disconnected);
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("[ChannelBridge] Read stdout error: {e}");
                    let _ = status_tx.send(ChannelStatus::Error(e.to_string()));
                    break;
                }
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Parse JSON-RPC message
            let value: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("[ChannelBridge] Invalid JSON from plugin: {e} — line: {trimmed}");
                    continue;
                }
            };

            let is_notification = value.get("id")
                .map(|id| id.is_null())
                .unwrap_or(true);

            if is_notification {
                // It's a notification (event pushed from plugin)
                Self::dispatch_notification(&value, &msg_tx, &status_tx, &ready_notify);
            } else {
                // It's a response to a pending request
                let id = value["id"].as_u64().unwrap_or(0);
                let mut map = pending.lock().unwrap();
                if let Some(tx) = map.remove(&id) {
                    if let Some(error) = value.get("error") {
                        let msg = error["message"].as_str().unwrap_or("unknown error");
                        let _ = tx.send(Err(CoreError::Feishu(format!("Plugin RPC error: {msg}"))));
                    } else {
                        let result = value.get("result").cloned().unwrap_or(serde_json::Value::Null);
                        let _ = tx.send(Ok(result));
                    }
                } else {
                    tracing::warn!("[ChannelBridge] No pending request found for id={id}");
                }
            }
        }

        tracing::debug!("[ChannelBridge] Read loop ended");
    }

    /// Dispatch a JSON-RPC notification to the appropriate channel.
    fn dispatch_notification(
        value: &serde_json::Value,
        msg_tx: &broadcast::Sender<NormalizedMessage>,
        status_tx: &tokio::sync::watch::Sender<ChannelStatus>,
        ready_notify: &Notify,
    ) {
        let method = value["method"].as_str().unwrap_or("");
        let params = &value["params"];

        match method {
            "feishu:message" => {
                if let Ok(msg) = serde_json::from_value::<NormalizedMessage>(params.clone()) {
                    tracing::debug!("[ChannelBridge] Message from {}: {}",
                        msg.sender_id, &msg.content[..msg.content.len().min(60)]);
                    let _ = msg_tx.send(msg);
                } else {
                    tracing::warn!("[ChannelBridge] Failed to parse NormalizedMessage: {params}");
                }
            }
            "feishu:ready" => {
                let bot_name = params["botName"].as_str().unwrap_or("unknown");
                tracing::info!("[ChannelBridge] Channel ready as: {bot_name}");
                let _ = status_tx.send(ChannelStatus::Connected {
                    bot_name: bot_name.to_string(),
                });
                ready_notify.notify_waiters();
            }
            "feishu:reconnecting" => {
                tracing::info!("[ChannelBridge] Reconnecting...");
                let _ = status_tx.send(ChannelStatus::Connecting);
            }
            "feishu:reconnected" => {
                tracing::info!("[ChannelBridge] Reconnected");
                let _ = status_tx.send(ChannelStatus::Connected {
                    bot_name: params["botName"].as_str().unwrap_or("unknown").to_string(),
                });
            }
            "feishu:error" => {
                let msg = params["message"].as_str().unwrap_or("unknown error");
                tracing::error!("[ChannelBridge] Plugin error: {msg}");
                let _ = status_tx.send(ChannelStatus::Error(msg.to_string()));
            }
            "feishu:card_action" => {
                tracing::debug!("[ChannelBridge] Card action received");
                // Future: dispatch card actions to agent handlers
            }
            "feishu:reaction" => {
                tracing::debug!("[ChannelBridge] Reaction event received");
            }
            "feishu:bot_added" => {
                tracing::info!("[ChannelBridge] Bot added to new group");
            }
            _ => {
                tracing::debug!("[ChannelBridge] Unknown notification: {method}");
            }
        }
    }
}

impl Default for FeishuChannelBridge {
    fn default() -> Self {
        Self::new()
    }
}