//! kasugai_box 自身の MCP サーバー（POST /mcp、Streamable HTTP / JSON-RPC 2.0）。
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::box_api::{
    box_api_get, create_shared_link, ensure_access_token, run_box_api_chat, search_box,
};
use crate::config::load_config;
use crate::AppState;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

fn rpc_result(id: Value, result: Value) -> Json<Value> {
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Json<Value> {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    }))
}

fn tool_text(id: Value, text: String, is_error: bool) -> Json<Value> {
    rpc_result(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "isError": is_error
        }),
    )
}

fn tools_definition() -> Value {
    json!([
        {
            "name": "box_whoami",
            "description": "Box にログイン中のユーザー情報を取得します。",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "box_search",
            "description": "Box 標準検索（ファイル名・メタデータ・文書内テキスト）を実行し、パスとリンクの一覧を返します。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "検索語" },
                    "limit": { "type": "integer", "description": "取得件数（1〜200、既定 100）" },
                    "offset": { "type": "integer", "description": "取得開始位置（既定 0）" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "box_list_folder",
            "description": "Box フォルダ内のアイテムを一覧します。folder_id 省略時はルート（0）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder_id": { "type": "string", "description": "Box フォルダ ID（既定 0）" }
                }
            }
        },
        {
            "name": "box_create_shared_link",
            "description": "Box のファイルまたはフォルダの共有リンクを作成します。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "item_type": { "type": "string", "enum": ["file", "folder"] },
                    "id": { "type": "string", "description": "ファイルまたはフォルダの ID" }
                },
                "required": ["item_type", "id"]
            }
        },
        {
            "name": "photos_process",
            "description": "Box フォルダ内の画像の EXIF から緯度経度・撮影日を抽出し CSV/GeoJSON を出力するジョブを開始します。job_status で進捗を確認してください。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder_url": { "type": "string", "description": "Box フォルダ URL または ID" },
                    "output_dir": { "type": "string", "description": "出力フォルダ（既定 c:/kasugai/box/photo）" }
                },
                "required": ["folder_url"]
            }
        },
        {
            "name": "job_status",
            "description": "photos_process などの長時間ジョブの進捗・結果を取得します。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"]
            }
        }
    ])
}

async fn call_tool(state: &Arc<AppState>, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "box_whoami" => {
            let mut config = load_config();
            let token = ensure_access_token(&mut config).await.map_err(|e| e.to_string())?;
            box_api_get(&token, "https://api.box.com/2.0/users/me")
                .await
                .map_err(|e| e.to_string())
        }
        "box_search" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or("query は必須です")?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 200) as usize;
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let mut config = load_config();
            let token = ensure_access_token(&mut config).await.map_err(|e| e.to_string())?;
            let result = search_box(&token, query, offset, limit)
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!(
                "{}\n\n{}-{}/{} 件",
                result.reply,
                result.offset + 1,
                result.display_end,
                result.total_count
            ))
        }
        "box_list_folder" => {
            let folder_id = args.get("folder_id").and_then(Value::as_str).unwrap_or("0");
            run_box_api_chat(format!("folder {}", folder_id), None)
                .await
                .map(|r| r.reply)
                .map_err(|e| e.to_string())
        }
        "box_create_shared_link" => {
            let item_type = args
                .get("item_type")
                .and_then(Value::as_str)
                .ok_or("item_type は必須です")?;
            let id = args.get("id").and_then(Value::as_str).ok_or("id は必須です")?;
            if item_type != "file" && item_type != "folder" {
                return Err("item_type には file または folder を指定してください".into());
            }
            let mut config = load_config();
            let token = ensure_access_token(&mut config).await.map_err(|e| e.to_string())?;
            create_shared_link(&token, item_type, id)
                .await
                .map_err(|e| e.to_string())
        }
        "photos_process" => {
            let folder_url = args
                .get("folder_url")
                .and_then(Value::as_str)
                .ok_or("folder_url は必須です")?
                .to_string();
            let output_dir = args
                .get("output_dir")
                .and_then(Value::as_str)
                .unwrap_or("c:/kasugai/box/photo")
                .to_string();
            let job_id = crate::start_photos_job(state.clone(), folder_url, output_dir)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "jobId": job_id, "status": "queued" }).to_string())
        }
        "job_status" => {
            let job_id = args.get("job_id").and_then(Value::as_str).ok_or("job_id は必須です")?;
            let job = state.jobs.get(job_id).ok_or("ジョブが見つかりません")?;
            serde_json::to_string_pretty(&job).map_err(|e| e.to_string())
        }
        _ => Err(format!("未知のツールです: {}", name)),
    }
}

pub async fn handle(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let id = body.get("id").cloned();

    // 通知（id なし）は 202 Accepted で受理する
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };

    match method {
        "initialize" => rpc_result(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "kasugai_box",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
        .into_response(),
        "ping" => rpc_result(id, json!({})).into_response(),
        "tools/list" => rpc_result(id, json!({ "tools": tools_definition() })).into_response(),
        "tools/call" => {
            let params = body.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let default_args = json!({});
            let args = params.get("arguments").unwrap_or(&default_args);
            match call_tool(&state, name, args).await {
                Ok(text) => tool_text(id, text, false).into_response(),
                Err(err) => tool_text(id, err, true).into_response(),
            }
        }
        _ => rpc_error(id, -32601, &format!("Method not found: {}", method)).into_response(),
    }
}
