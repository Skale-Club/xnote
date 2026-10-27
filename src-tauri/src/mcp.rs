//! Local MCP server. `<app>.exe --mcp` opens no window: it speaks the Model
//! Context Protocol over stdin/stdout (one JSON-RPC 2.0 message per line), so
//! Claude Code, Codex and other MCP clients can use the app's own engine on
//! this computer. Nothing goes over the network. Each app lists its tools in
//! `mcp_tools.rs`; this file is the same in every Skale app, keep the copies
//! in step.

// Not every app uses every helper.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema of the arguments.
    pub input: Value,
    /// False for tools that change something (they never delete anything).
    pub read_only: bool,
    pub run: Box<dyn Fn(&Value) -> Result<Value, String>>,
}

impl Tool {
    pub fn new(
        name: &'static str,
        description: &'static str,
        input: Value,
        run: impl Fn(&Value) -> Result<Value, String> + 'static,
    ) -> Self {
        Self { name, description, input, read_only: true, run: Box::new(run) }
    }

    pub fn changing(mut self) -> Self {
        self.read_only = false;
        self
    }
}

/// No arguments.
pub fn no_input() -> Value {
    json!({ "type": "object", "properties": {} })
}

pub fn requested() -> bool {
    std::env::args().skip(1).any(|a| a == "--mcp")
}

/// A required text argument.
pub fn text(args: &Value, key: &str) -> Result<String, String> {
    match args.get(key).and_then(Value::as_str).map(str::trim) {
        Some(value) if !value.is_empty() => Ok(value.to_string()),
        _ => Err(format!("Missing \"{key}\".")),
    }
}

pub fn optional_text(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

pub fn number(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(Value::as_u64).unwrap_or(default)
}

pub fn flag(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Keeps the first `limit` items of `value[key]` and says how many were left out.
pub fn trim_list(value: &mut Value, key: &str, limit: usize) {
    if let Some(list) = value.get_mut(key).and_then(Value::as_array_mut) {
        let more = list.len().saturating_sub(limit);
        list.truncate(limit);
        if more > 0 {
            value[format!("{key}NotShown")] = json!(more);
        }
    }
}

/// What every app answers to `app_info`: where it runs and its settings.
fn app_info(name: &str, settings_file: &str) -> Value {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let data = crate::portable::exe_dir().join("Data");
    let settings = std::fs::read_to_string(data.join(settings_file))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok());
    json!({
        "name": name,
        "version": env!("CARGO_PKG_VERSION"),
        "exe": exe,
        "dataFolder": data.display().to_string(),
        "settings": settings,
    })
}

/// Answers requests until the client closes stdin.
pub fn serve(name: &'static str, settings_file: &'static str, instructions: &str, mut tools: Vec<Tool>) {
    tools.insert(
        0,
        Tool::new(
            "app_info",
            "The app's version, where its exe and Data folder are, and its saved settings.",
            no_input(),
            move |_| Ok(app_info(name, settings_file)),
        ),
    );
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(&message, name, instructions, &tools),
            Err(e) => Some(error(Value::Null, -32700, &format!("Parse error: {e}"))),
        };
        if let Some(reply) = reply {
            if writeln!(stdout, "{reply}").and_then(|_| stdout.flush()).is_err() {
                break;
            }
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The reply to one message; None for notifications.
fn handle(message: &Value, name: &str, instructions: &str, tools: &[Tool]) -> Option<Value> {
    let id = message.get("id")?.clone();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match message.get("method").and_then(Value::as_str).unwrap_or("") {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = PROTOCOLS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOLS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": name.to_lowercase(), "title": name, "version": env!("CARGO_PKG_VERSION") },
                "instructions": instructions,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({
            "tools": tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input,
                "annotations": {
                    "readOnlyHint": t.read_only,
                    "destructiveHint": false,
                    "openWorldHint": false,
                },
            })).collect::<Vec<_>>()
        }),
        "tools/call" => {
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let Some(tool) = tools.iter().find(|t| t.name == tool_name) else {
                return Some(error(id, -32602, &format!("Unknown tool: {tool_name}")));
            };
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match (tool.run)(&args) {
                Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
                Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
            }
        }
        method => return Some(error(id, -32601, &format!("Method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<Tool> {
        vec![
            Tool::new("echo", "Echoes.", no_input(), |args| Ok(args.clone())),
            Tool::new("fail", "Fails.", no_input(), |_| Err("nope".into())),
        ]
    }

    fn call(method: &str, params: Value) -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
        handle(&message, "Test", "", &tools()).unwrap()
    }

    #[test]
    fn negotiates_the_protocol() {
        let reply = call("initialize", json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        let reply = call("initialize", json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(reply["result"]["protocolVersion"], PROTOCOLS[0]);
        assert_eq!(reply["id"], 7);
    }

    #[test]
    fn calls_tools_and_reports_failures() {
        assert_eq!(call("tools/list", json!({}))["result"]["tools"].as_array().unwrap().len(), 2);
        let ok = call("tools/call", json!({ "name": "echo", "arguments": { "a": 1 } }));
        assert_eq!(ok["result"]["content"][0]["text"], "{\"a\":1}");
        let failed = call("tools/call", json!({ "name": "fail" }));
        assert_eq!(failed["result"]["isError"], true);
        assert_eq!(call("tools/call", json!({ "name": "nope" }))["error"]["code"], -32602);
        assert_eq!(call("nope", json!({}))["error"]["code"], -32601);
    }

    #[test]
    fn notifications_get_no_reply() {
        let message = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle(&message, "Test", "", &tools()).is_none());
    }

    #[test]
    fn trims_long_lists() {
        let mut value = json!({ "files": [1, 2, 3, 4] });
        trim_list(&mut value, "files", 2);
        assert_eq!(value, json!({ "files": [1, 2], "filesNotShown": 2 }));
    }
}
