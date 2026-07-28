use anyhow::{Context, Result};
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{save_config, Config};

#[derive(Deserialize, Debug)]
pub struct BoxSharedLink {
    pub url: String,
    #[allow(dead_code)]
    pub access: String,
}

#[derive(Deserialize, Debug)]
pub struct BoxPathEntry {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct BoxPathCollection {
    pub entries: Vec<BoxPathEntry>,
}

#[derive(Deserialize, Debug)]
pub struct BoxItem {
    pub r#type: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub shared_link: Option<BoxSharedLink>,
    #[serde(default)]
    pub path_collection: Option<BoxPathCollection>,
}

#[derive(Deserialize, Debug)]
pub struct BoxFolderItems {
    pub entries: Vec<BoxItem>,
    pub offset: Option<usize>,
    pub total_count: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct BoxUser {
    pub name: String,
    pub login: String,
    pub r#type: String,
}

#[derive(Deserialize, Debug)]
struct BoxSearchResponse {
    entries: Vec<BoxItem>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    total_count: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

pub const OAUTH_REDIRECT_URI: &str = "http://localhost:8000/callback";
pub const OAUTH_REDIRECT_URI_ENCODED: &str = "http%3A%2F%2Flocalhost%3A8000%2Fcallback";

pub fn generate_state() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{}", std::process::id(), timestamp)
}

pub fn extract_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

pub async fn ensure_access_token(config: &mut Config) -> Result<String> {
    if let Some(token) = config.developer_token.as_ref() {
        return Ok(token.clone());
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    if let Some(token) = config.access_token.as_ref() {
        if let Some(exp) = config.expires_at {
            if now < exp - 60 {
                return Ok(token.clone());
            }
        }
    }

    let client_id = config.client_id.as_deref().context("クライアントIDが設定されていません")?;
    let client_secret = config.client_secret.as_deref().context("クライアントシークレットが設定されていません")?;

    let token = if let Some(refresh) = config.refresh_token.as_ref() {
        let tokens = refresh_oauth_token(client_id, client_secret, refresh).await?;
        let expires_at = now + tokens.expires_in as i64;
        config.access_token = Some(tokens.access_token.clone());
        config.refresh_token = Some(tokens.refresh_token);
        config.expires_at = Some(expires_at);
        save_config(config)?;
        tokens.access_token
    } else {
        return Err(anyhow::anyhow!("OAuth ログインが必要です"));
    };

    Ok(token)
}

pub async fn get_oauth_token(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthTokenResponse> {
    let mut params = std::collections::HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("client_id", client_id);
    params.insert("client_secret", client_secret);
    params.insert("code", code);
    params.insert("redirect_uri", redirect_uri);

    let resp = Client::new()
        .post("https://api.box.com/oauth2/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Box OAuth token error {}: {}", status, text));
    }

    Ok(resp.json().await?)
}

pub async fn refresh_oauth_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<OAuthTokenResponse> {
    let mut params = std::collections::HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("client_id", client_id);
    params.insert("client_secret", client_secret);
    params.insert("refresh_token", refresh_token);

    let resp = Client::new()
        .post("https://api.box.com/oauth2/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Box OAuth refresh error {}: {}", status, text));
    }

    Ok(resp.json().await?)
}

pub async fn box_api_get(token: &str, url: &str) -> Result<String> {
    let client = Client::builder()
        .default_headers({
            let mut h = header::HeaderMap::new();
            h.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&format!("Bearer {}", token))?,
            );
            h
        })
        .build()?;
    let resp = client.get(url).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("Box API error {}: {}", status, text));
    }
    Ok(text)
}

pub async fn box_api_put(token: &str, url: &str, body: &str) -> Result<String> {
    let client = Client::builder()
        .default_headers({
            let mut h = header::HeaderMap::new();
            h.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&format!("Bearer {}", token))?,
            );
            h.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/json"),
            );
            h
        })
        .build()?;
    let resp = client.put(url).body(body.to_string()).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow::anyhow!("Box API error {}: {}", status, text));
    }
    Ok(text)
}

#[derive(Deserialize, Debug)]
struct RepStatus {
    state: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RepInfo {
    url: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RepContent {
    url_template: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RepresentationEntry {
    representation: String,
    status: Option<RepStatus>,
    info: Option<RepInfo>,
    content: Option<RepContent>,
}

#[derive(Deserialize, Debug)]
struct Representations {
    entries: Vec<RepresentationEntry>,
}

#[derive(Deserialize, Debug)]
struct FileRepresentations {
    representations: Option<Representations>,
}

/// `x-rep-hints` ヘッダー名（representation の指定に必須）。
const REP_HINTS: &str = "x-rep-hints";
/// 埋め込みメタデータ（ExifTool 形式の JSON）の representation 名。
const EMBEDDED_METADATA_HINT: &str = "[embedded_metadata]";

/// ファイルの `embedded_metadata` 表現を取得し、JSON 形式の埋め込みメタデータを返す。
///
/// Box の representation は ondemand 生成のため、`state` が `none` の場合は
/// `info.url` を GET して生成をトリガーし、`success`/`viewable` になるまでポーリングする。
/// `client` は Authorization ヘッダーを既定ヘッダーに持つものを渡す。
pub async fn fetch_embedded_metadata(client: &Client, file_id: &str) -> Result<serde_json::Value> {
    let url = format!(
        "https://api.box.com/2.0/files/{}?fields=representations",
        file_id
    );

    let mut triggered = false;
    for attempt in 0..30 {
        let resp = client
            .get(&url)
            .header(REP_HINTS, EMBEDDED_METADATA_HINT)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Box representation list error {}: {}", status, text));
        }
        let file: FileRepresentations = resp.json().await?;
        let rep = file
            .representations
            .as_ref()
            .and_then(|r| r.entries.iter().find(|e| e.representation == "embedded_metadata"))
            .context("embedded_metadata 表現が利用できません（未対応のファイル形式の可能性）")?;

        let state = rep
            .status
            .as_ref()
            .and_then(|s| s.state.as_deref())
            .unwrap_or("none");

        match state {
            "success" | "viewable" => {
                let template = rep
                    .content
                    .as_ref()
                    .and_then(|c| c.url_template.as_ref())
                    .context("埋め込みメタデータの URL テンプレートがありません")?;
                let download_url = template.replace("{+asset_path}", "");
                let meta_resp = client.get(&download_url).send().await?;
                if !meta_resp.status().is_success() {
                    let status = meta_resp.status();
                    let text = meta_resp.text().await.unwrap_or_default();
                    return Err(anyhow::anyhow!(
                        "Box embedded metadata download error {}: {}",
                        status,
                        text
                    ));
                }
                let bytes = meta_resp.bytes().await?;
                return serde_json::from_slice(&bytes)
                    .context("埋め込みメタデータの JSON パースに失敗しました");
            }
            "error" => {
                let status = rep.status.as_ref();
                return Err(anyhow::anyhow!(
                    "Box embedded metadata 生成エラー: {} {}",
                    status.and_then(|s| s.code.as_deref()).unwrap_or(""),
                    status.and_then(|s| s.message.as_deref()).unwrap_or("")
                ));
            }
            "none" if !triggered => {
                if let Some(info_url) = rep.info.as_ref().and_then(|i| i.url.as_ref()) {
                    let _ = client.get(info_url).send().await?;
                }
                triggered = true;
            }
            _ => {}
        }

        let wait = if attempt < 4 { 300 } else { 1000 };
        tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
    }

    Err(anyhow::anyhow!("Box embedded metadata 生成がタイムアウトしました"))
}

#[derive(Clone)]
struct SearchState {
    query: String,
    next_offset: usize,
    limit: usize,
}

pub struct SearchResult {
    pub reply: String,
    pub offset: usize,
    pub display_end: usize,
    pub next_offset: usize,
    pub total_count: usize,
    pub has_more: bool,
}

static LAST_SEARCH: Mutex<Option<SearchState>> = Mutex::new(None);

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub async fn search_box(token: &str, query: &str, offset: usize, limit: usize) -> Result<SearchResult> {
    let mut url = reqwest::Url::parse("https://api.box.com/2.0/search")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("query", query);
        pairs.append_pair("limit", &limit.to_string());
        pairs.append_pair("offset", &offset.to_string());
        pairs.append_pair("fields", "type,id,name,path_collection,shared_link");
    }
    let body = box_api_get(token, url.as_str()).await?;
    let data: BoxSearchResponse = serde_json::from_str(&body)?;
    let response_offset = data.offset.unwrap_or(offset);
    let total_count = data.total_count.unwrap_or(0);
    let display_end = std::cmp::min(response_offset + data.entries.len(), total_count);
    let next_offset = response_offset + limit;
    let has_more = next_offset < total_count;
    let reply = if data.entries.is_empty() {
        "検索結果が見つかりませんでした。".to_string()
    } else {
        data.entries
            .iter()
            .map(|i| {
                let path = i
                    .path_collection
                    .as_ref()
                    .map(|pc| {
                        let parents: Vec<&str> = pc.entries.iter().skip(1).map(|e| e.name.as_str()).collect();
                        if parents.is_empty() {
                            i.name.clone()
                        } else {
                            parents.join("/") + "/" + &i.name
                        }
                    })
                    .unwrap_or_else(|| i.name.clone());
                let url = i
                    .shared_link
                    .as_ref()
                    .map(|l| l.url.clone())
                    .unwrap_or_else(|| {
                        if i.r#type == "folder" {
                            format!("https://app.box.com/folder/{}", i.id)
                        } else {
                            format!("https://app.box.com/file/{}", i.id)
                        }
                    });
                let safe_url = escape_html(&url);
                format!("<a href=\"{}\" target=\"_blank\">{}</a>", safe_url, escape_html(&path))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(SearchResult {
        reply,
        offset: response_offset,
        display_end,
        next_offset,
        total_count,
        has_more,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxApiChatResponse {
    pub reply: String,
    pub has_more: bool,
}

pub async fn run_box_api_chat(text: String, search_limit: Option<usize>) -> Result<BoxApiChatResponse> {
    let mut config = crate::config::load_config();
    let token = ensure_access_token(&mut config).await?;
    let limit = search_limit.or(config.search_limit).unwrap_or(100);
    let parts: Vec<&str> = text.trim().splitn(2, ' ').collect();
    let cmd = parts.first().copied().unwrap_or("").to_lowercase();
    let arg = parts.get(1).copied().unwrap_or("").trim();

    let reply;
    let mut has_more = false;
    let mut next_state: Option<SearchState> = None;

    match cmd.as_str() {
        "help" | "?" | "" => {
            reply = "利用可能なコマンド:\n".to_string()
                + "  help        このヘルプを表示\n"
                + "  me          ログインユーザー情報を表示\n"
                + "  folder <id> フォルダ内のアイテムを一覧\n"
                + "  search <q>  Box標準検索（ファイル名・メタデータ・文書内テキスト）\n"
                + "  link <type> <id> ファイルまたはフォルダの共有リンクを作成\n"
                + "  ルール: コマンドと引数は半角スペースで区切ってください。全角スペースは検索語の一部になります。\n"
                + "  検索結果は 100 件ずつ取得し、Enter で次の 100 件を読み込みます。（設定で 1〜200 件に変更可）";
        }
        "me" | "user" => {
            let body = box_api_get(&token, "https://api.box.com/2.0/users/me").await?;
            let user: BoxUser = serde_json::from_str(&body)?;
            reply = format!("ユーザー: {} ({} / {})", user.name, user.login, user.r#type);
        }
        "folder" => {
            let id = if arg.is_empty() { "0".to_string() } else { arg.to_string() };
            let url = format!(
                "https://api.box.com/2.0/folders/{}/items?limit=20&fields=type,id,name",
                id
            );
            let body = box_api_get(&token, &url).await?;
            let data: BoxFolderItems = serde_json::from_str(&body)?;
            if data.entries.is_empty() {
                reply = "アイテムが見つかりませんでした。".to_string();
            } else {
                reply = data.entries
                    .iter()
                    .map(|i| format!("[{}] {} ({})", i.r#type, i.name, i.id))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        "search" => {
            if arg.is_empty() {
                reply = "検索語を入力してください。".to_string();
            } else {
                let result = search_box(&token, arg, 0, limit).await?;
                let footer = format!(
                    "{}-{}/{} 件{}",
                    result.offset + 1,
                    result.display_end,
                    result.total_count,
                    if result.has_more { "（Enter で続き）" } else { "（すべて表示）" }
                );
                reply = format!("{}\n\n{}", result.reply, footer);
                has_more = result.has_more;
                next_state = Some(SearchState {
                    query: arg.to_string(),
                    next_offset: result.next_offset,
                    limit,
                });
            }
        }
        "more" => {
            let last = LAST_SEARCH.lock().unwrap().clone();
            if let Some(state) = last {
                let result = search_box(&token, &state.query, state.next_offset, state.limit).await?;
                let footer = format!(
                    "{}-{}/{} 件{}",
                    result.offset + 1,
                    result.display_end,
                    result.total_count,
                    if result.has_more { "（Enter で続き）" } else { "（すべて表示）" }
                );
                reply = format!("{}\n\n{}", result.reply, footer);
                has_more = result.has_more;
                next_state = Some(SearchState {
                    query: state.query,
                    next_offset: result.next_offset,
                    limit: state.limit,
                });
            } else {
                reply = "先に search を実行してください。".to_string();
            }
        }
        "link" => {
            let parts: Vec<&str> = arg.split_whitespace().collect();
            if parts.len() != 2 {
                reply = "使用方法: link <file|folder> <id>".to_string();
            } else if parts[0] != "file" && parts[0] != "folder" {
                reply = "type には file または folder を指定してください。".to_string();
            } else {
                reply = create_shared_link(&token, parts[0], parts[1]).await?;
            }
        }
        _ => {
            let query = text.trim();
            if query.is_empty() {
                reply = "検索語を入力してください。".to_string();
            } else {
                let result = search_box(&token, query, 0, limit).await?;
                let footer = format!(
                    "{}-{}/{} 件{}",
                    result.offset + 1,
                    result.display_end,
                    result.total_count,
                    if result.has_more { "（Enter で続き）" } else { "（すべて表示）" }
                );
                reply = format!("{}\n\n{}", result.reply, footer);
                has_more = result.has_more;
                next_state = Some(SearchState {
                    query: query.to_string(),
                    next_offset: result.next_offset,
                    limit,
                });
            }
        }
    }

    if let Some(state) = next_state {
        *LAST_SEARCH.lock().unwrap() = Some(state);
    }

    Ok(BoxApiChatResponse { reply, has_more })
}

pub async fn create_shared_link(token: &str, item_type: &str, id: &str) -> Result<String> {
    let url = format!("https://api.box.com/2.0/{}s/{}", item_type.trim_end_matches('s'), id);
    let payload = r#"{"shared_link": {"access": "open"}}"#;
    let body = box_api_put(token, &url, payload).await?;
    let item: BoxItem = serde_json::from_str(&body)?;
    Ok(if let Some(link) = item.shared_link {
        format!("{} の共有リンク: {}", item.name, link.url)
    } else {
        "共有リンクを取得できませんでした。".to_string()
    })
}
