#![windows_subsystem = "windows"]
mod auth;
mod box_api;
mod config;
mod jobs;
mod mcp_client;
mod mcp_server;
mod photos;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Notify;

use crate::box_api::ensure_access_token;
use crate::config::{apply_update, load_config, save_config, ConfigUpdate, ConfigView, DEFAULT_PORT};
use crate::jobs::Jobs;

pub struct AppState {
    pub jobs: Jobs,
    pub port: u16,
    pub shutdown: Arc<Notify>,
}

const INDEX_HTML: &str = include_str!("../../web/index.html");
const MAIN_JS: &str = include_str!("../../web/main.js");
const STYLES_CSS: &str = include_str!("../../web/styles.css");
const OPENAPI_YAML: &str = include_str!("../../openapi.yaml");
const FAVICON_ICO: &[u8] = include_bytes!("../../web/favicon.ico");

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        ApiError { status: StatusCode::BAD_REQUEST, message: message.into() }
    }
    fn not_found(message: impl Into<String>) -> Self {
        ApiError { status: StatusCode::NOT_FOUND, message: message.into() }
    }
}

impl<E: std::fmt::Display> From<E> for ApiError {
    fn from(err: E) -> Self {
        ApiError { status: StatusCode::INTERNAL_SERVER_ERROR, message: err.to_string() }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "name": "kasugai_box",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

const LATEST_JSON_URL: &str = "https://yamamoto-ryuzo.github.io/kasugai_box/download/latest.json";

async fn fetch_latest() -> ApiResult<serde_json::Value> {
    let response = reqwest::get(LATEST_JSON_URL).await?.error_for_status()?;
    let text = response.text().await?;
    let data: serde_json::Value = serde_json::from_str(&text)?;
    Ok(data)
}

async fn update_latest() -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(fetch_latest().await?))
}

async fn install_update(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let data = fetch_latest().await?;
    let url = data["platforms"]["windows-x86_64"]["url"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("ダウンロードURLが見つかりません"))?;

    let current_exe = std::env::current_exe().map_err(ApiError::from)?;
    let parent_pid = std::process::id();
    let tmp_dir = std::env::temp_dir().join(format!("kasugai_box_update_{parent_pid}"));
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(ApiError::from)?;

    let zip_path = tmp_dir.join("kasugai_box.zip");
    let extract_dir = tmp_dir.join("extracted");

    let response = reqwest::get(url).await?.error_for_status()?;
    let bytes = response.bytes().await?;
    tokio::fs::write(&zip_path, bytes).await.map_err(ApiError::from)?;

    let extract_status = tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Expand-Archive",
            "-Path",
            &zip_path.to_string_lossy(),
            "-DestinationPath",
            &extract_dir.to_string_lossy(),
            "-Force",
        ])
        .status()
        .await
        .map_err(ApiError::from)?;
    if !extract_status.success() {
        return Err(ApiError::bad_request("ZIP展開に失敗しました"));
    }

    let new_exe = extract_dir.join("kasugai_box.exe");
    if !new_exe.exists() {
        return Err(ApiError::bad_request("展開後に実行ファイルが見つかりません"));
    }

    let script_path = tmp_dir.join("update.ps1");
    let script = format!(
        "$parentPid = {parent_pid}\n$newExe = '{new}'\n$currentExe = '{current}'\nwhile (Get-Process -Id $parentPid -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 500 }}\nCopy-Item -Path $newExe -Destination $currentExe -Force\nStart-Process -FilePath $currentExe -WindowStyle Hidden\n",
        new = new_exe.to_string_lossy().replace("'", "''"),
        current = current_exe.to_string_lossy().replace("'", "''")
    );
    tokio::fs::write(&script_path, script).await.map_err(ApiError::from)?;

    tokio::process::Command::new("powershell")
        .args([
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .spawn()
        .map_err(ApiError::from)?;

    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        state.shutdown.notify_one();
    });

    Ok(Json(json!({ "message": "アップデートを開始しました。数秒後に再起動します。" })))
}

async fn get_config() -> Json<ConfigView> {
    Json(ConfigView::from(&load_config()))
}

async fn post_config(Json(update): Json<ConfigUpdate>) -> ApiResult<Json<ConfigView>> {
    let config = apply_update(update).map_err(ApiError::from)?;
    Ok(Json(ConfigView::from(&config)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    client_id: Option<String>,
    client_secret: Option<String>,
}

async fn auth_login(Json(req): Json<LoginRequest>) -> ApiResult<Json<serde_json::Value>> {
    let message = auth::box_oauth_login(req.client_id, req.client_secret).await?;
    Ok(Json(json!({ "message": message })))
}

#[derive(Deserialize)]
struct DeveloperTokenRequest {
    token: String,
}

async fn auth_developer_token(Json(req): Json<DeveloperTokenRequest>) -> ApiResult<Json<serde_json::Value>> {
    if req.token.trim().is_empty() {
        return Err(ApiError::bad_request("デベロッパートークンを入力してください"));
    }
    let message = auth::developer_token_login(req.token.trim().to_string()).await?;
    Ok(Json(json!({ "message": message })))
}

async fn auth_status() -> Json<serde_json::Value> {
    let config = load_config();
    Json(json!({
        "loggedIn": config.access_token.is_some() || config.developer_token.is_some(),
        "expiresAt": config.expires_at
    }))
}

async fn auth_logout() -> ApiResult<Json<serde_json::Value>> {
    let mut config = load_config();
    config.access_token = None;
    config.refresh_token = None;
    config.expires_at = None;
    config.developer_token = None;
    save_config(&config)?;
    Ok(Json(json!({ "message": "ログアウトしました" })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhotosProcessRequest {
    folder_url: String,
    output_dir: Option<String>,
}

/// 写真処理ジョブを開始し、ジョブ ID を返す（REST・MCP 共用）。
pub async fn start_photos_job(
    state: Arc<AppState>,
    folder_url: String,
    output_dir: String,
) -> Result<String> {
    let mut config = load_config();
    config.folder_url = Some(folder_url.clone());
    save_config(&config)?;
    let token = ensure_access_token(&mut config).await?;

    let job_id = state.jobs.create();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        state.jobs.set_running(&job_id_clone);
        let jobs = &state.jobs;
        let progress_id = job_id_clone.clone();
        let result = photos::run_process(token, folder_url, output_dir, |p| {
            jobs.set_progress(&progress_id, p);
        })
        .await;
        match result {
            Ok(res) => match serde_json::to_value(&res) {
                Ok(value) => state.jobs.complete(&job_id_clone, value),
                Err(e) => state.jobs.fail(&job_id_clone, e.to_string()),
            },
            Err(e) => state.jobs.fail(&job_id_clone, e.to_string()),
        }
    });
    Ok(job_id)
}

async fn photos_process(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PhotosProcessRequest>,
) -> ApiResult<Response> {
    if req.folder_url.trim().is_empty() {
        return Err(ApiError::bad_request("folderUrl を入力してください"));
    }
    let output_dir = req
        .output_dir
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "c:/kasugai/box/photo".to_string());
    let job_id = start_photos_job(state, req.folder_url.trim().to_string(), output_dir).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "jobId": job_id, "status": "queued" })),
    )
        .into_response())
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<jobs::JobInfo>> {
    state
        .jobs
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("ジョブが見つかりません"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    text: String,
    search_limit: Option<usize>,
}

async fn box_chat(Json(req): Json<ChatRequest>) -> ApiResult<Json<box_api::BoxApiChatResponse>> {
    let response = box_api::run_box_api_chat(req.text, req.search_limit).await?;
    Ok(Json(response))
}

async fn mcp_client_list_tools() -> ApiResult<Json<serde_json::Value>> {
    let text = mcp_client::list_tools().await.map_err(ApiError::from)?;
    Ok(Json(json!({ "result": text })))
}

#[derive(Deserialize)]
struct McpCallRequest {
    name: String,
    #[serde(default)]
    arguments: String,
}

async fn mcp_client_call_tool(Json(req): Json<McpCallRequest>) -> ApiResult<Json<serde_json::Value>> {
    let text = mcp_client::call_tool(req.name, req.arguments)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "result": text })))
}

async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn serve_main_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], MAIN_JS)
}

async fn serve_styles_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLES_CSS)
}

async fn serve_openapi() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/yaml; charset=utf-8")], OPENAPI_YAML)
}

async fn serve_favicon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/x-icon")], FAVICON_ICO)
}

async fn server_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "running": true,
        "port": state.port,
        "bind": "127.0.0.1"
    }))
}

async fn server_stop(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    // レスポンスを確実に返した上で graceful shutdown を開始する
    state.shutdown.notify_one();
    Json(json!({ "message": "kasugai_box を停止します" }))
}

#[tokio::main]
async fn main() {
    let saved_config = load_config();
    let port = std::env::var("KASUGAI_BOX_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .or(saved_config.port)
        .unwrap_or(DEFAULT_PORT);

    let shutdown = Arc::new(Notify::new());
    let state = Arc::new(AppState {
        jobs: Jobs::default(),
        port,
        shutdown: shutdown.clone(),
    });

    let app = Router::new()
        .route("/", get(|| async { Redirect::to("/ui") }))
        .route("/ui", get(serve_index))
        .route("/main.js", get(serve_main_js))
        .route("/styles.css", get(serve_styles_css))
        .route("/health", get(health))
        .route("/api/v1/update/latest", get(update_latest))
        .route("/api/v1/update/install", post(install_update))
        .route("/openapi.yaml", get(serve_openapi))
        .route("/favicon.ico", get(serve_favicon))
        .route("/api/v1/config", get(get_config).post(post_config))
        .route("/api/v1/auth/box/login", post(auth_login))
        .route("/api/v1/auth/box/developer-token", post(auth_developer_token))
        .route("/api/v1/auth/box/status", get(auth_status))
        .route("/api/v1/auth/box/logout", post(auth_logout))
        .route("/api/v1/photos/process", post(photos_process))
        .route("/api/v1/jobs/{id}", get(get_job))
        .route("/api/v1/box/chat", post(box_chat))
        .route("/api/v1/mcp-client/tools", get(mcp_client_list_tools))
        .route("/api/v1/mcp-client/call", post(mcp_client_call_tool))
        .route("/api/v1/server/status", get(server_status))
        .route("/api/v1/server/stop", post(server_stop))
        .route("/mcp", post(mcp_server::handle))
        .with_state(state);

    let open_browser = std::env::args().any(|a| a == "--open-browser");

    // 127.0.0.1 固定（方針書 1.2：0.0.0.0 バインド禁止）
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            // 既に起動済み（ポート使用中）の場合は、多重起動せず既存インスタンスの UI をブラウザで開いて終了する
            let health_url = format!("http://127.0.0.1:{}/health", port);
            if reqwest::get(&health_url)
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                let _ = opener::open(format!("http://127.0.0.1:{}/ui", port));
                println!("kasugai_box は既にポート {} で起動しています。ブラウザで UI を開きました。", port);
                return;
            }
            eprintln!("kasugai_box: ポート {} で起動できません: {}", port, e);
            eprintln!("環境変数 KASUGAI_BOX_PORT で別のポートを指定してください。");
            std::process::exit(1);
        }
    };
    println!(
        "kasugai_box v{} が http://127.0.0.1:{} で起動しました（UI: /ui, API: /api/v1, MCP: /mcp）",
        env!("CARGO_PKG_VERSION"),
        port
    );
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        shutdown.notified().await;
    });
    if open_browser {
        let open_url = format!("http://127.0.0.1:{}/ui", port);
        let health_url = format!("http://127.0.0.1:{}/health", port);
        tokio::spawn(async move {
            for _ in 0..60 {
                if let Ok(resp) = reqwest::get(&health_url).await {
                    if resp.status().is_success() {
                        let _ = opener::open(&open_url);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });
    }
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    println!("kasugai_box を停止しました");
}
