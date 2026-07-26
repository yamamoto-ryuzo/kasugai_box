//! 外部 MCP サーバーへ接続するクライアント（UI の「MCP」タブ用）。
use reqwest::{header, Client};

use crate::box_api::ensure_access_token;
use crate::config::load_config;

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

fn parse_mcp_body(content_type: &str, text: &str) -> String {
    if content_type.contains("text/event-stream") {
        let mut last = None;
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if !data.is_empty() {
                    last = Some(data.to_string());
                }
            }
        }
        last.unwrap_or_else(|| text.to_string())
    } else {
        text.to_string()
    }
}

async fn mcp_send(
    client: &Client,
    url: &str,
    token: Option<&str>,
    session_id: Option<&str>,
    body: serde_json::Value,
) -> Result<(reqwest::StatusCode, Option<String>, String), String> {
    let mut req = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .json(&body);
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {}", token));
    }
    if let Some(sid) = session_id {
        req = req.header("Mcp-Session-Id", sid);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let new_session = resp
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok((status, new_session, parse_mcp_body(&content_type, &text)))
}

async fn mcp_rpc(method: &str, params: serde_json::Value) -> Result<String, String> {
    let mut config = load_config();
    let url = config
        .mcp_server_url
        .clone()
        .ok_or("MCP サーバーURLが設定されていません")?;
    let url = url.as_str();
    let token = match config.mcp_connection_token.clone() {
        Some(t) if !t.trim().is_empty() => Some(t),
        _ => ensure_access_token(&mut config).await.ok(),
    };
    let token = token.as_deref();
    let client = Client::new();

    let init_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "kasugai_box",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    });
    let (status, session_id, init_text) = mcp_send(&client, url, token, None, init_body).await?;
    if !status.is_success() {
        return Err(format!("MCP initialize エラー {}: {}", status, init_text));
    }

    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = mcp_send(&client, url, token, session_id.as_deref(), notif).await;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let (status, _, text) = mcp_send(&client, url, token, session_id.as_deref(), body).await?;
    if !status.is_success() {
        return Err(format!("MCP エラー {}: {}", status, text));
    }
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => Ok(serde_json::to_string_pretty(&v).unwrap_or(text)),
        Err(_) => Ok(text),
    }
}

pub async fn list_tools() -> Result<String, String> {
    mcp_rpc("tools/list", serde_json::json!({})).await
}

pub async fn call_tool(name: String, arguments: String) -> Result<String, String> {
    let args: serde_json::Value = if arguments.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&arguments).map_err(|e| format!("引数JSONのパースエラー: {}", e))?
    };
    mcp_rpc(
        "tools/call",
        serde_json::json!({
            "name": name,
            "arguments": args
        }),
    )
    .await
}
