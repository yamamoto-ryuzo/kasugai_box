use anyhow::{Context, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::box_api::{
    box_api_get, extract_query_param, generate_state, get_oauth_token, BoxUser,
    OAUTH_REDIRECT_URI, OAUTH_REDIRECT_URI_ENCODED,
};
use crate::config::{load_config, save_config};

const CALLBACK_HTML: &str = "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><title>kasugai_box</title></head><body><p>ログインが完了しました。このタブを閉じて kasugai_box に戻ってください。</p></body></html>";

/// OAuth コールバック（http://localhost:8000/callback）を一時的に待ち受け、認可コードを受け取る。
async fn wait_for_oauth_code(expected_state: &str) -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:8000")
        .await
        .context("ポート 8000 を使用できません（他のプロセスが使用中の可能性があります）")?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow::anyhow!("Box OAuth ログインがタイムアウトしました"));
        }
        let (mut stream, _) = tokio::time::timeout(remaining, listener.accept())
            .await
            .map_err(|_| anyhow::anyhow!("Box OAuth ログインがタイムアウトしました"))??;

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("");

        if let Some(query) = path.strip_prefix("/callback?") {
            let state = extract_query_param(query, "state");
            let code = extract_query_param(query, "code");
            if state.as_deref() == Some(expected_state) {
                if let Some(code) = code {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        CALLBACK_HTML.len(),
                        CALLBACK_HTML
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    return Ok(code);
                }
            }
        }
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
    }
}

/// システムブラウザで Box OAuth ログインを行い、トークンを keyring に保存する。
pub async fn box_oauth_login(client_id: Option<String>, client_secret: Option<String>) -> Result<String> {
    let mut config = load_config();
    if let Some(id) = client_id {
        if !id.trim().is_empty() {
            config.client_id = Some(id.trim().to_string());
        }
    }
    if let Some(secret) = client_secret {
        if !secret.trim().is_empty() {
            config.client_secret = Some(secret.trim().to_string());
        }
    }
    let client_id = config
        .client_id
        .clone()
        .context("クライアントIDが設定されていません")?;
    let client_secret = config
        .client_secret
        .clone()
        .context("クライアントシークレットが設定されていません")?;
    save_config(&config)?;

    let state = generate_state();
    let auth_url = format!(
        "https://account.box.com/api/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&state={}",
        client_id, OAUTH_REDIRECT_URI_ENCODED, state
    );

    opener::open_browser(&auth_url).context("ブラウザを起動できませんでした")?;

    let code = wait_for_oauth_code(&state).await?;

    let tokens = get_oauth_token(&client_id, &client_secret, &code, OAUTH_REDIRECT_URI).await?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let mut config = load_config();
    config.client_id = Some(client_id);
    config.client_secret = Some(client_secret);
    config.access_token = Some(tokens.access_token.clone());
    config.refresh_token = Some(tokens.refresh_token);
    config.expires_at = Some(now + tokens.expires_in as i64);
    save_config(&config)?;

    let body = box_api_get(&tokens.access_token, "https://api.box.com/2.0/users/me").await?;
    let user: BoxUser = serde_json::from_str(&body)?;
    Ok(format!("{} ({}) としてログインしました", user.name, user.login))
}

pub async fn developer_token_login(token: String) -> Result<String> {
    let body = box_api_get(&token, "https://api.box.com/2.0/users/me").await?;
    let user: BoxUser = serde_json::from_str(&body)?;
    let mut config = load_config();
    config.developer_token = Some(token);
    save_config(&config)?;
    Ok(format!("{} ({}) としてログインしました（デベロッパートークン）", user.name, user.login))
}
