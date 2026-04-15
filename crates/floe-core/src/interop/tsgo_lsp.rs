//! Minimal LSP client for querying types from tsgo.
//!
//! Spawns `tsgo --lsp --stdio` as a child process, communicates over stdio
//! using JSON-RPC, and provides type queries via `textDocument/hover`.
//! Uses a background reader thread so reads never block indefinitely.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

/// A minimal LSP client that queries types from tsgo.
pub struct TsgoLspClient {
    process: Child,
    stdin: ChildStdin,
    /// Receives parsed JSON-RPC messages from the reader thread.
    rx: mpsc::Receiver<Value>,
    next_id: i64,
    /// Cache: (file_path, symbol_name) → hover type string
    cache: HashMap<(PathBuf, String), Option<String>>,
    /// Timeout for waiting for LSP responses.
    timeout: Duration,
}

impl TsgoLspClient {
    /// Spawn `tsgo --lsp --stdio` and perform the initialization handshake.
    pub fn new(project_dir: &Path) -> Result<Self, String> {
        let tsgo = find_tsgo()?;

        let mut process = Command::new(&tsgo)
            .args(["--lsp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn tsgo --lsp: {e}"))?;

        let stdin = process.stdin.take().ok_or("no stdin")?;
        let stdout = process.stdout.take().ok_or("no stdout")?;

        // Spawn a background thread that reads LSP messages and sends them
        // through a channel. This prevents blocking the main thread.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(msg) = read_lsp_message(&mut reader) {
                if tx.send(msg).is_err() {
                    break; // receiver dropped
                }
            }
        });

        let mut client = Self {
            process,
            stdin,
            rx,
            next_id: 1,
            cache: HashMap::new(),
            timeout: Duration::from_secs(10),
        };

        // Initialize handshake
        let root_uri = format!("file://{}", project_dir.display());
        let _init_result = client.send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "hover": {
                            "contentFormat": ["plaintext"]
                        }
                    }
                },
            }),
        )?;

        client.send_notification("initialized", json!({}))?;

        Ok(client)
    }

    /// Query the type of a symbol at a specific position in a file.
    /// Returns the hover content as a string, or None if no hover info.
    pub fn hover(&mut self, file_path: &Path, line: u32, character: u32) -> Option<String> {
        let uri = path_to_uri(file_path);
        let result = self
            .send_request(
                "textDocument/hover",
                json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character },
                }),
            )
            .ok()?;

        if result.is_null() {
            return None;
        }

        // Extract the content from the hover response
        let contents = &result["contents"];
        contents["value"]
            .as_str()
            .or_else(|| contents.as_str())
            .map(|s| s.to_string())
    }

    /// Open a document in the LSP server.
    pub fn open_document(&mut self, file_path: &Path) -> Result<(), String> {
        let uri = path_to_uri(file_path);
        let content = std::fs::read_to_string(file_path).map_err(|e| format!("read error: {e}"))?;

        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": if file_path.extension().is_some_and(|e| e == "tsx") {
                        "typescriptreact"
                    } else {
                        "typescript"
                    },
                    "version": 1,
                    "text": content,
                }
            }),
        )
    }

    /// Close a document in the LSP server.
    pub fn close_document(&mut self, file_path: &Path) -> Result<(), String> {
        let uri = path_to_uri(file_path);
        self.send_notification(
            "textDocument/didClose",
            json!({
                "textDocument": { "uri": uri }
            }),
        )
    }

    /// Open a document from content already read (avoids double file read).
    fn open_document_with_content(
        &mut self,
        file_path: &Path,
        content: &str,
    ) -> Result<(), String> {
        let uri = path_to_uri(file_path);
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": if file_path.extension().is_some_and(|e| e == "tsx") {
                        "typescriptreact"
                    } else {
                        "typescript"
                    },
                    "version": 1,
                    "text": content,
                }
            }),
        )
    }

    /// Query the hover type for a symbol name at its declaration in a source file.
    /// Uses a cache to avoid repeated queries for the same symbol.
    pub fn query_symbol_type(&mut self, file_path: &Path, symbol_name: &str) -> Option<String> {
        let cache_key = (file_path.to_path_buf(), symbol_name.to_string());
        if let Some(cached) = self.cache.get(&cache_key) {
            return cached.clone();
        }

        // Read file once — used for both symbol search and didOpen
        let content = std::fs::read_to_string(file_path).ok()?;
        let pos = find_symbol_position(&content, symbol_name)?;

        // Ensure the document is open
        self.open_document_with_content(file_path, &content).ok()?;

        let result = self.hover(file_path, pos.0, pos.1);
        self.cache.insert(cache_key, result.clone());
        result
    }

    /// Shut down the LSP server.
    pub fn shutdown(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }

    // ── JSON-RPC transport ─────────────────────────────────────

    fn send_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.send_message(&message)?;
        self.read_response(id)
    }

    fn send_notification(&mut self, method: &str, params: Value) -> Result<(), String> {
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send_message(&message)
    }

    fn send_message(&mut self, message: &Value) -> Result<(), String> {
        let body = serde_json::to_string(message).map_err(|e| format!("json error: {e}"))?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        self.stdin
            .write_all(header.as_bytes())
            .map_err(|e| format!("write error: {e}"))?;
        self.stdin
            .write_all(body.as_bytes())
            .map_err(|e| format!("write error: {e}"))?;
        self.stdin
            .flush()
            .map_err(|e| format!("flush error: {e}"))?;

        Ok(())
    }

    fn read_response(&mut self, expected_id: i64) -> Result<Value, String> {
        let deadline = std::time::Instant::now() + self.timeout;

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!(
                    "timeout waiting for response to request {expected_id}"
                ));
            }

            let message = self
                .rx
                .recv_timeout(remaining)
                .map_err(|e| format!("recv error: {e}"))?;

            // Skip notifications (no id)
            if message.get("id").is_none() {
                continue;
            }

            // Handle server-initiated requests (e.g. client/registerCapability)
            // by sending an empty success response.
            if message.get("method").is_some() {
                let server_id = &message["id"];
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": server_id,
                    "result": null,
                });
                let _ = self.send_message(&response);
                continue;
            }

            if message["id"].as_i64() == Some(expected_id) {
                if let Some(error) = message.get("error") {
                    return Err(format!("LSP error: {}", error));
                }
                return Ok(message["result"].clone());
            }
        }
    }
}

impl Drop for TsgoLspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ── LSP message parsing (runs in background thread) ────────────

/// Read a single LSP message (Content-Length header + JSON body).
fn read_lsp_message(reader: &mut BufReader<impl Read>) -> Result<Value, String> {
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("read error: {e}"))?;

        if line.is_empty() {
            return Err("EOF".to_string());
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str
                .parse()
                .map_err(|e| format!("invalid content length: {e}"))?;
        }
    }

    if content_length == 0 {
        return Err("no Content-Length header".to_string());
    }

    let mut body = vec![0u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("read error: {e}"))?;

    serde_json::from_slice(&body).map_err(|e| format!("json parse error: {e}"))
}

// ── Helpers ────────────────────────────────────────────────────

/// Convert a file path to a file:// URI.
fn path_to_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Find the tsgo binary.
fn find_tsgo() -> Result<PathBuf, String> {
    if let Ok(output) = Command::new("which").arg("tsgo").output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    Err("tsgo not found — install with: npm i -g @typescript/native-preview".to_string())
}

/// Find the (line, character) position of a symbol declaration in file content.
fn find_symbol_position(content: &str, symbol_name: &str) -> Option<(u32, u32)> {
    let patterns = [
        format!("function {symbol_name}"),
        format!("const {symbol_name}"),
        format!("let {symbol_name}"),
        format!("var {symbol_name}"),
        format!("interface {symbol_name}"),
        format!("type {symbol_name}"),
        format!("class {symbol_name}"),
        format!("enum {symbol_name}"),
    ];

    for (line_num, line) in content.lines().enumerate() {
        for pattern in &patterns {
            if let Some(col) = line.find(pattern.as_str()) {
                let name_col = col + pattern.len() - symbol_name.len();
                return Some((line_num as u32, name_col as u32));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_symbol_in_content() {
        let content = "import { foo } from 'bar';\nexport function useMemo<T>(factory: () => T): T;\nconst x = 1;";
        let pos = find_symbol_position(content, "useMemo");
        assert_eq!(pos, Some((1, 16)));
    }

    #[test]
    fn find_interface_symbol() {
        let content = "interface DropResult extends DragUpdate {\n    reason: DropReason;\n}";
        let pos = find_symbol_position(content, "DropResult");
        assert_eq!(pos, Some((0, 10)));
    }

    #[test]
    fn lsp_initialize_and_hover() {
        let todo_app_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/todo-app");
        if !todo_app_dir.join("node_modules").is_dir() {
            eprintln!("Skipping: no node_modules in todo-app");
            return;
        }

        let mut client = match TsgoLspClient::new(&todo_app_dir) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping: {e}");
                return;
            }
        };

        // Query a real .ts file
        let types_file = todo_app_dir.join("src/types.ts");
        if types_file.exists() {
            let result = client.query_symbol_type(&types_file, "Todo");
            eprintln!("Todo hover: {result:?}");
            assert!(result.is_some(), "expected hover for Todo type");
        }
    }
}
