use std::path::Path;
use std::sync::OnceLock;
use crate::feishu::bridge::FeishuBridge;
use crate::feishu::types::{MessagePriority, OutgoingMessage};
use crate::CoreError;
use super::config::ToolDefinition;

/// Configuration for Feishu integration (bridge + chat_id)
pub struct FeishuConfig {
    pub bridge: FeishuBridge,
    pub chat_id: String,
}

/// Global static to hold the FeishuConfig for use by send_feishu_message tool
static FEISHU_SENDER: OnceLock<tokio::sync::Mutex<Option<FeishuConfig>>> = OnceLock::new();

/// Register the FeishuBridge with chat_id for use by the send_feishu_message tool
pub fn register_feishu_bridge(bridge: FeishuBridge, chat_id: String) {
    let lock = FEISHU_SENDER.get_or_init(|| tokio::sync::Mutex::new(None));
    *lock.blocking_lock() = Some(FeishuConfig { bridge, chat_id });
    tracing::info!("[builtin] FeishuBridge registered with chat_id for send_feishu_message tool");
}

/// Returns the list of built-in tool definitions (always available to all agents)
pub fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from the local filesystem. Returns the content as text (max 100KB). Only works for UTF-8 encoded files.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Absolute path to the file"}
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_file".into(),
            description: "Write text content to a file. Creates parent directories if they don't exist.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Absolute path to the file"},
                    "content": {"type": "string", "description": "Content to write"}
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "glob_files".into(),
            description: "Find files matching a glob pattern (e.g., 'src/**/*.rs'). Returns up to 100 matches.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern to match"},
                    "root": {"type": "string", "description": "Root directory to search from (default: current dir)"}
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "grep_search".into(),
            description: "Search file contents using a regex pattern. Returns up to 50 matching lines with file paths.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex pattern to search"},
                    "include": {"type": "string", "description": "File glob to filter (e.g., '*.rs')"},
                    "path": {"type": "string", "description": "Directory to search in"}
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "list_directory".into(),
            description: "List all entries (files and subdirectories) in a directory.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path"}
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "bash_exec".into(),
            description: "Execute a shell command and return stdout + stderr. Has a 10-second timeout. For Windows, uses PowerShell.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"}
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "web_fetch".into(),
            description: "Fetch content from a URL and return it as text/markdown.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch"}
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "send_feishu_message".into(),
            description: "Send a message to the Feishu group chat. Use this ONLY when you need to communicate results to the user, request collaboration from another agent via @mention, or escalate an issue you cannot resolve yourself. Do NOT use this for internal reasoning or intermediate thoughts.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string", "description": "The message content to send"},
                    "mention_agents": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional list of agent names to @mention"
                    }
                },
                "required": ["message"]
            }),
        },
    ]
}

/// Check if a tool name is a built-in tool
pub fn is_builtin(name: &str) -> bool {
    matches!(name, "read_file" | "write_file" | "glob_files" | "grep_search" | "list_directory" | "bash_exec" | "web_fetch" | "send_feishu_message")
}

/// Execute a built-in tool and return the result text
pub async fn execute_builtin(name: &str, args: &serde_json::Value) -> Result<String, CoreError> {
    match name {
        "read_file" => cmd_read_file(args),
        "write_file" => cmd_write_file(args).await,
        "glob_files" => cmd_glob_files(args),
        "grep_search" => cmd_grep_search(args),
        "list_directory" => cmd_list_directory(args),
        "bash_exec" => cmd_bash_exec(args).await,
        "web_fetch" => cmd_web_fetch(args).await,
        "send_feishu_message" => cmd_send_feishu(args).await,
        _ => Err(CoreError::Mcp(format!("Unknown built-in tool: {name}"))),
    }
}

fn cmd_read_file(args: &serde_json::Value) -> Result<String, CoreError> {
    let path = args["path"].as_str().ok_or_else(|| CoreError::Mcp("read_file requires 'path' argument".into()))?;
    let metadata = std::fs::metadata(path)
        .map_err(|e| CoreError::Mcp(format!("Cannot access {path}: {e}")))?;
    if metadata.len() > 102_400 {
        return Err(CoreError::Mcp(format!("File too large ({}) — max 100KB", metadata.len())));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| CoreError::Mcp(format!("Read {path}: {e}")))?;
    Ok(format!("```\n{}```\n\n({} bytes, {} lines)", content, metadata.len(), content.lines().count()))
}

async fn cmd_write_file(args: &serde_json::Value) -> Result<String, CoreError> {
    let path = args["path"].as_str().ok_or_else(|| CoreError::Mcp("write_file requires 'path'".into()))?;
    let content = args["content"].as_str().ok_or_else(|| CoreError::Mcp("write_file requires 'content'".into()))?;
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CoreError::Mcp(format!("Create dirs for {path}: {e}")))?;
    }
    // Use tokio::fs::write for async file I/O
    tokio::fs::write(path, content).await
        .map_err(|e| CoreError::Mcp(format!("Write {path}: {e}")))?;
    Ok(format!("Written {} bytes to {}", content.len(), path))
}

fn cmd_glob_files(args: &serde_json::Value) -> Result<String, CoreError> {
    let pattern = args["pattern"].as_str().ok_or_else(|| CoreError::Mcp("glob_files requires 'pattern'".into()))?;
    let root = args["root"].as_str().unwrap_or(".");
    // Use globset or walkdir — for now use a simple recursive search via std::fs
    let mut results = Vec::new();
    let root_path = Path::new(root);
    if root_path.is_dir() {
        walk_dir(root_path, pattern, &mut results, 0)?;
    }
    results.truncate(100);
    if results.is_empty() {
        Ok("No files found matching pattern".into())
    } else {
        Ok(format!("Found {} files:\n{}", results.len(), results.join("\n")))
    }
}

fn walk_dir(dir: &Path, pattern: &str, results: &mut Vec<String>, depth: usize) -> Result<(), CoreError> {
    if depth > 5 || results.len() >= 100 { return Ok(()); }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_dir(&path, pattern, results, depth + 1)?;
            } else if let Some(name) = path.to_str() {
                if glob_match(pattern, name) {
                    results.push(name.to_string());
                }
            }
        }
    }
    Ok(())
}

fn glob_match(pattern: &str, path: &str) -> bool {
    // Simple glob matching: convert glob to regex
    let regex_str = pattern
        .replace('.', "\\.")
        .replace('*', ".*")
        .replace('?', ".");
    if let Ok(re) = regex::Regex::new(&format!("^{regex_str}$")) {
        re.is_match(path)
    } else {
        false
    }
}

fn cmd_grep_search(args: &serde_json::Value) -> Result<String, CoreError> {
    let pattern = args["pattern"].as_str().ok_or_else(|| CoreError::Mcp("grep_search requires 'pattern'".into()))?;
    let include = args["include"].as_str().unwrap_or("*");
    let search_path = args["path"].as_str().unwrap_or(".");
    let re = regex::Regex::new(pattern)
        .map_err(|e| CoreError::Mcp(format!("Invalid regex '{pattern}': {e}")))?;

    let mut results = Vec::new();
    let root = Path::new(search_path);
    if root.is_dir() {
        grep_dir(root, &re, include, &mut results, 0)?;
    }
    results.truncate(50);
    if results.is_empty() {
        Ok("No matches found".into())
    } else {
        Ok(format!("Found {} matches:\n{}", results.len(), results.join("\n")))
    }
}

fn grep_dir(dir: &Path, re: &regex::Regex, include: &str, results: &mut Vec<String>, depth: usize) -> Result<(), CoreError> {
    if depth > 5 || results.len() >= 50 { return Ok(()); }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                grep_dir(&path, re, include, results, depth + 1)?;
            } else if let Some(name) = path.to_str() {
                if glob_match(include, name) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for (i, line) in content.lines().enumerate() {
                            if re.is_match(line) {
                                results.push(format!("{}:{}: {}", name, i + 1, line.trim()));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_list_directory(args: &serde_json::Value) -> Result<String, CoreError> {
    let path = args["path"].as_str().ok_or_else(|| CoreError::Mcp("list_directory requires 'path'".into()))?;
    let dir = Path::new(path);
    if !dir.is_dir() {
        return Err(CoreError::Mcp(format!("Not a directory: {path}")));
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| CoreError::Mcp(format!("Read dir {path}: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| {
            let kind = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { "dir" } else { "file" };
            format!("{}  {}", kind, e.file_name().to_string_lossy())
        })
        .collect();
    entries.sort();
    Ok(format!("{} entries in {}:\n{}", entries.len(), path, entries.join("\n")))
}

async fn cmd_bash_exec(args: &serde_json::Value) -> Result<String, CoreError> {
    let command = args["command"].as_str().ok_or_else(|| CoreError::Mcp("bash_exec requires 'command'".into()))?;

    // Use cmd.exe on Windows, sh on Unix
    let shell = if cfg!(windows) { "cmd.exe" } else { "sh" };
    let flag = if cfg!(windows) { "/c" } else { "-c" };

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new(shell)
            .arg(flag)
            .arg(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| CoreError::Mcp("bash_exec timed out after 10s".into()))?
    .map_err(|e| CoreError::Mcp(format!("bash_exec failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&format!("stdout:\n{stdout}\n"));
    }
    if !stderr.is_empty() {
        result.push_str(&format!("stderr:\n{stderr}\n"));
    }
    if !output.status.success() {
        result.push_str(&format!("exit code: {}", output.status.code().unwrap_or(-1)));
    }
    Ok(result.trim().to_string())
}

async fn cmd_web_fetch(args: &serde_json::Value) -> Result<String, CoreError> {
    let url = args["url"].as_str().ok_or_else(|| CoreError::Mcp("web_fetch requires 'url'".into()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("FeishuAgentOrchestrator/1.0")
        .build()
        .map_err(|e| CoreError::Mcp(format!("HTTP client: {e}")))?;

    let response = client.get(url).send().await
        .map_err(|e| CoreError::Mcp(format!("Fetch {url}: {e}")))?;

    let status = response.status();
    let text = response.text().await
        .map_err(|e| CoreError::Mcp(format!("Read response: {e}")))?;

    let preview = &text[..text.len().min(5000)];
    Ok(format!("HTTP {status}\n\n{preview}"))
}

async fn cmd_send_feishu(args: &serde_json::Value) -> Result<String, CoreError> {
    let message = args["message"].as_str()
        .ok_or_else(|| CoreError::Mcp("send_feishu_message requires 'message'".into()))?;

    let lock = FEISHU_SENDER.get_or_init(|| tokio::sync::Mutex::new(None));
    let guard = lock.lock().await;
    let config = guard.as_ref()
        .ok_or_else(|| CoreError::Mcp("FeishuBridge not initialized".into()))?;

    let outgoing = OutgoingMessage {
        chat_id: config.chat_id.clone(),
        thread_id: None,
        text: message.to_string(),
        mentions: vec![],
        priority: MessagePriority::Secretary,
    };

    let msg_id = config.bridge.send_message(&outgoing).await
        .map_err(|e| CoreError::Mcp(format!("Feishu send failed: {e}")))?;

    tracing::info!("[send_feishu] Sent to Feishu: message_id={msg_id}");
    Ok(format!("Message sent to Feishu (id: {msg_id})"))
}
