use anyhow::{Context, Result};
use exif::{In, Reader, Tag, Value};
use keyring::Entry;
use futures::future::BoxFuture;
use futures::FutureExt;
use regex::Regex;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager, Url};
use tauri::webview::WebviewWindowBuilder;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub folder_url: Option<String>,
    pub developer_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub mcp_server_url: Option<String>,
    pub mcp_connection_token: Option<String>,
}

#[derive(Deserialize, Debug)]
struct BoxSharedLink {
    url: String,
    access: String,
}

#[derive(Deserialize, Debug)]
struct BoxPathEntry {
    name: String,
}

#[derive(Deserialize, Debug)]
struct BoxPathCollection {
    entries: Vec<BoxPathEntry>,
}

#[derive(Deserialize, Debug)]
struct BoxItem {
    r#type: String,
    id: String,
    name: String,
    #[serde(default)]
    shared_link: Option<BoxSharedLink>,
    #[serde(default)]
    path_collection: Option<BoxPathCollection>,
}

#[derive(Deserialize, Debug)]
struct BoxFolderItems {
    entries: Vec<BoxItem>,
    offset: Option<usize>,
    total_count: Option<usize>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhotoRecord {
    pub name: String,
    pub full_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub date_taken: Option<String>,
    pub url: String,
}

#[derive(Deserialize, Debug)]
struct BoxUser {
    name: String,
    login: String,
    r#type: String,
}

#[derive(Deserialize, Debug)]
struct BoxSearchResponse {
    entries: Vec<BoxItem>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    total_count: Option<usize>,
}

async fn ensure_access_token(config: &mut Config) -> Result<String> {
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

#[derive(Deserialize, Debug)]
pub struct OAuthTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub records: Vec<PhotoRecord>,
    pub csv_path: String,
    pub geojson_path: Option<String>,
    pub message: String,
}

const KEYRING_SERVICE: &str = "kasugai_box";
const KEYRING_ACCOUNT: &str = "kasugai_box_config";

fn load_config() -> Config {
    match Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(entry) => match entry.get_password() {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        },
        Err(_) => Config::default(),
    }
}

fn save_config(config: &Config) -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)?;
    let content = serde_json::to_string_pretty(config)?;
    entry.set_password(&content)?;
    Ok(())
}

fn resolve_output_dir(output_dir: &str) -> PathBuf {
    let path = PathBuf::from(output_dir);
    if path.is_absolute() {
        path
    } else {
        dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(path)
    }
}

fn extract_folder_id(url: &str) -> String {
    let re = Regex::new(r"/folder/(\d+)").unwrap();
    if let Some(caps) = re.captures(url) {
        caps[1].to_string()
    } else {
        url.to_string()
    }
}

fn get_exif_location_and_datetime(bytes: &[u8]) -> (Option<f64>, Option<f64>, Option<String>) {
    let mut cursor = std::io::Cursor::new(bytes);
    let reader = Reader::new();
    let exif = match reader.read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(_) => return (None, None, None),
    };

    let get_coord = |tag, ref_tag| -> Option<f64> {
        let field = exif.get_field(tag, In::PRIMARY)?;
        let ref_field = exif.get_field(ref_tag, In::PRIMARY)?;

        let coords = match &field.value {
            Value::Rational(r) if r.len() == 3 => r,
            _ => return None,
        };

        let d = coords[0].to_f64();
        let m = coords[1].to_f64();
        let s = coords[2].to_f64();
        let mut deg = d + (m / 60.0) + (s / 3600.0);

        if let Value::Ascii(arr) = &ref_field.value {
            if let Some(dir_arr) = arr.first() {
                if let Some(dir) = dir_arr.first() {
                    if *dir == b'S' || *dir == b'W' {
                        deg = -deg;
                    }
                }
            }
        }
        Some(deg)
    };

    let lat = get_coord(Tag::GPSLatitude, Tag::GPSLatitudeRef);
    let lon = get_coord(Tag::GPSLongitude, Tag::GPSLongitudeRef);

    let date_taken = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::DateTime, In::PRIMARY))
        .map(|f| f.display_value().with_unit(&exif).to_string());

    (lat, lon, date_taken)
}

fn get_image_files_recursive<'a>(
    client: &'a Client,
    folder_id: &'a str,
    parent_path: &'a str,
) -> BoxFuture<'a, Result<Vec<(BoxItem, String)>>> {
    async move {
        let mut image_files = Vec::new();
        let mut offset = 0;
        let limit = 1000;

        loop {
            let url = format!(
                "https://api.box.com/2.0/folders/{}/items?limit={}&offset={}&fields=type,id,name",
                folder_id, limit, offset
            );

            let resp = client.get(&url).send().await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!("Box API Error {}: {}", status, text));
            }

            let data: BoxFolderItems = resp.json().await?;
            let count = data.entries.len();

            for item in data.entries {
                if item.r#type == "file" {
                    let ext = item.name.to_lowercase();
                    if ext.ends_with(".jpg")
                        || ext.ends_with(".jpeg")
                        || ext.ends_with(".tif")
                        || ext.ends_with(".tiff")
                        || ext.ends_with(".heic")
                    {
                        image_files.push((item, parent_path.to_string()));
                    }
                } else if item.r#type == "folder" {
                    let current_path = if parent_path.is_empty() {
                        item.name.clone()
                    } else {
                        format!("{}/{}", parent_path, item.name)
                    };
                    let mut sub_files =
                        get_image_files_recursive(client, &item.id, &current_path).await?;
                    image_files.append(&mut sub_files);
                }
            }

            if count < limit
                || data.offset.unwrap_or(0) + count >= data.total_count.unwrap_or(0)
            {
                break;
            }
            offset += limit;
        }
        Ok(image_files)
    }
    .boxed()
}

fn create_geojson(file_path: &Path, records: &[PhotoRecord]) -> Result<()> {
    let mut features = Vec::new();
    for r in records {
        if let (Some(lat), Some(lon)) = (r.latitude, r.longitude) {
            let feature = serde_json::json!({
                "type": "Feature",
                "geometry": {
                    "type": "Point",
                    "coordinates": [lon, lat]
                },
                "properties": {
                    "name": &r.name,
                    "full_name": &r.full_name,
                    "url": &r.url,
                    "date_taken": &r.date_taken
                }
            });
            features.push(feature);
        }
    }

    let feature_collection = serde_json::json!({
        "type": "FeatureCollection",
        "features": features
    });

    let file = fs::File::create(file_path)?;
    serde_json::to_writer(file, &feature_collection)?;
    Ok(())
}

async fn run_process(
    token: String,
    folder_url: String,
    output_dir: String,
) -> Result<ProcessResult> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", token))?,
    );

    let client = Client::builder().default_headers(headers).build()?;
    let folder_id = extract_folder_id(&folder_url);

    let image_files = get_image_files_recursive(&client, &folder_id, "").await?;

    let mut records = Vec::new();

    for (file, parent_path) in image_files {
        let content_url = format!("https://api.box.com/2.0/files/{}/content", file.id);

        let bytes = match client.get(&content_url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => b.to_vec(),
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let (lat, lon, date_taken) = get_exif_location_and_datetime(&bytes);
        let url = format!("https://app.box.com/file/{}", file.id);
        let full_name = if parent_path.is_empty() {
            file.name.clone()
        } else {
            format!("{}/{}", parent_path, file.name)
        };

        records.push(PhotoRecord {
            name: file.name,
            full_name,
            latitude: lat,
            longitude: lon,
            date_taken,
            url,
        });
    }

    let output = resolve_output_dir(&output_dir);
    fs::create_dir_all(&output).with_context(|| format!("出力フォルダを作成できません: {}", output_dir))?;

    let csv_path = output.join("box_photos.csv");
    {
        let mut wtr = csv::Writer::from_path(&csv_path)?;
        for r in &records {
            wtr.serialize(r)?;
        }
        wtr.flush()?;
    }

    let has_geom = records.iter().any(|r| r.latitude.is_some() && r.longitude.is_some());
    let geojson_path = if has_geom {
        let path = output.join("box_photos.geojson");
        create_geojson(&path, &records)?;
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };

    let message = if records.is_empty() {
        "対象ファイルが見つかりませんでした。".into()
    } else if geojson_path.is_some() {
        format!(
            "{}件の画像を処理しました。CSVとGeoJSONを出力しました。",
            records.len()
        )
    } else {
        format!("{}件の画像を処理しました。位置情報が含まれていませんでした。", records.len())
    };

    Ok(ProcessResult {
        records,
        csv_path: csv_path.to_string_lossy().to_string(),
        geojson_path,
        message,
    })
}

#[tauri::command]
fn load_saved_config() -> Config {
    load_config()
}

#[tauri::command]
fn save_config_cmd(config: Config) -> Result<(), String> {
    let mut existing = load_config();
    existing.client_id = config.client_id;
    existing.client_secret = config.client_secret;
    existing.folder_url = config.folder_url;
    existing.developer_token = config.developer_token.or(existing.developer_token);
    existing.access_token = config.access_token.or(existing.access_token);
    existing.refresh_token = config.refresh_token.or(existing.refresh_token);
    existing.expires_at = config.expires_at.or(existing.expires_at);
    existing.mcp_server_url = config.mcp_server_url.or(existing.mcp_server_url);
    existing.mcp_connection_token = config.mcp_connection_token.or(existing.mcp_connection_token);
    save_config(&existing).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxApiChatResponse {
    pub reply: String,
    pub has_more: bool,
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

async fn box_api_put(token: &str, url: &str, body: &str) -> Result<String> {
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

#[derive(Clone)]
struct SearchState {
    query: String,
    next_offset: usize,
}

struct SearchResult {
    reply: String,
    offset: usize,
    next_offset: usize,
    total_count: usize,
    has_more: bool,
}

static LAST_SEARCH: Mutex<Option<SearchState>> = Mutex::new(None);

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn search_box(token: &str, query: &str, offset: usize) -> Result<SearchResult> {
    let mut url = reqwest::Url::parse("https://api.box.com/2.0/search")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("query", query);
        pairs.append_pair("limit", "50");
        pairs.append_pair("offset", &offset.to_string());
        pairs.append_pair("fields", "type,id,name,path_collection,shared_link");
    }
    let body = box_api_get(token, url.as_str()).await?;
    let data: BoxSearchResponse = serde_json::from_str(&body)?;
    let response_offset = data.offset.unwrap_or(offset);
    let total_count = data.total_count.unwrap_or(0);
    let next_offset = response_offset + data.entries.len();
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
        next_offset,
        total_count,
        has_more,
    })
}

async fn run_box_api_chat(text: String) -> Result<BoxApiChatResponse> {
    let mut config = load_config();
    let token = ensure_access_token(&mut config).await?;
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
                + "  検索結果は 50 件ずつ取得し、Enter で次の 50 件を読み込みます。";
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
                let result = search_box(&token, arg, 0).await?;
                let footer = format!(
                    "{}-{}/{} 件{}",
                    result.offset + 1,
                    result.next_offset,
                    result.total_count,
                    if result.has_more { "（Enter で続き）" } else { "（すべて表示）" }
                );
                reply = format!("{}\n\n{}", result.reply, footer);
                has_more = result.has_more;
                next_state = Some(SearchState {
                    query: arg.to_string(),
                    next_offset: result.next_offset,
                });
            }
        }
        "more" => {
            let last = LAST_SEARCH.lock().unwrap().clone();
            if let Some(state) = last {
                let result = search_box(&token, &state.query, state.next_offset).await?;
                let footer = format!(
                    "{}-{}/{} 件{}",
                    result.offset + 1,
                    result.next_offset,
                    result.total_count,
                    if result.has_more { "（Enter で続き）" } else { "（すべて表示）" }
                );
                reply = format!("{}\n\n{}", result.reply, footer);
                has_more = result.has_more;
                next_state = Some(SearchState {
                    query: state.query,
                    next_offset: result.next_offset,
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
                let url = format!("https://api.box.com/2.0/{}/{}", parts[0], parts[1]);
                let payload = r#"{"shared_link": {"access": "open"}}"#;
                let body = box_api_put(&token, &url, payload).await?;
                let item: BoxItem = serde_json::from_str(&body)?;
                reply = if let Some(link) = item.shared_link {
                    format!("{} の共有リンク: {}", item.name, link.url)
                } else {
                    "共有リンクを取得できませんでした。".to_string()
                };
            }
        }
        _ => {
            let query = text.trim();
            if query.is_empty() {
                reply = "検索語を入力してください。".to_string();
            } else {
                let result = search_box(&token, query, 0).await?;
                let footer = format!(
                    "{}-{}/{} 件{}",
                    result.offset + 1,
                    result.next_offset,
                    result.total_count,
                    if result.has_more { "（Enter で続き）" } else { "（すべて表示）" }
                );
                reply = format!("{}\n\n{}", result.reply, footer);
                has_more = result.has_more;
                next_state = Some(SearchState {
                    query: query.to_string(),
                    next_offset: result.next_offset,
                });
            }
        }
    }

    if let Some(state) = next_state {
        *LAST_SEARCH.lock().unwrap() = Some(state);
    }

    Ok(BoxApiChatResponse { reply, has_more })
}

#[tauri::command]
async fn box_api_chat(text: String) -> Result<BoxApiChatResponse, String> {
    run_box_api_chat(text).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn process_photos(
    client_id: String,
    client_secret: String,
    folder_url: String,
    output_dir: String,
) -> Result<ProcessResult, String> {
    let mut config = load_config();
    config.client_id = Some(client_id);
    config.client_secret = Some(client_secret);
    config.folder_url = Some(folder_url.clone());
    save_config(&config).map_err(|e| e.to_string())?;
    let token = ensure_access_token(&mut config).await.map_err(|e| e.to_string())?;
    run_process(token, folder_url, output_dir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn box_api_get_cmd(token: String, url: String) -> Result<String, String> {
    box_api_get(&token, &url).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mcp_list_tools() -> Result<String, String> {
    let config = load_config();
    let url = config.mcp_server_url.as_deref().ok_or("MCP サーバーURLが設定されていません")?;
    let client = Client::new();
    let mut req = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }));
    if let Some(token) = config.mcp_connection_token.as_deref() {
        req = req.header(header::AUTHORIZATION, format!("Bearer {}", token));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

#[tauri::command]
async fn mcp_call_tool(name: String, arguments: String) -> Result<String, String> {
    let config = load_config();
    let url = config.mcp_server_url.as_deref().ok_or("MCP サーバーURLが設定されていません")?;
    let args: serde_json::Value = serde_json::from_str(&arguments)
        .map_err(|e| format!("引数JSONのパースエラー: {}", e))?;
    let client = Client::new();
    let mut req = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": args
            }
        }));
    if let Some(token) = config.mcp_connection_token.as_deref() {
        req = req.header(header::AUTHORIZATION, format!("Bearer {}", token));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthStatus {
    logged_in: bool,
    expires_at: Option<i64>,
}

#[tauri::command]
async fn box_oauth_login(app: AppHandle, client_id: String, client_secret: String) -> Result<String, String> {
    let state = generate_state();
    let auth_url = format!(
        "https://account.box.com/api/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&state={}",
        client_id, OAUTH_REDIRECT_URI_ENCODED, state
    );
    let url = auth_url.parse::<Url>().map_err(|e| e.to_string())?;

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let app_handle = app.clone();
    let expected_state = state.clone();

    let _window = WebviewWindowBuilder::new(&app, "oauth", tauri::WebviewUrl::External(url))
        .title("Box Login")
        .inner_size(800.0, 700.0)
        .on_navigation(move |url| {
            if url.scheme() == "http"
                && url.host_str() == Some("localhost")
                && url.port() == Some(8000)
                && url.path() == "/callback"
            {
                if let Some(query) = url.query() {
                    let state_param = extract_query_param(query, "state");
                    let code = extract_query_param(query, "code");
                    if state_param.as_deref() == Some(&expected_state) && code.is_some() {
                        if let Some(sender) = tx.lock().unwrap().take() {
                            let _ = sender.send(code.unwrap());
                        }
                        if let Some(win) = app_handle.get_webview_window("oauth") {
                            let _ = win.close();
                        }
                        return false;
                    }
                }
            }
            true
        })
        .build()
        .map_err(|e| e.to_string())?;

    let code = tokio::time::timeout(Duration::from_secs(120), rx)
        .await
        .map_err(|_| "Box OAuth ログインがタイムアウトしました".to_string())?
        .map_err(|_| "認可コードの受信に失敗しました".to_string())?;

    let tokens = get_oauth_token(&client_id, &client_secret, &code, OAUTH_REDIRECT_URI)
        .await
        .map_err(|e| e.to_string())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut config = load_config();
    config.client_id = Some(client_id);
    config.client_secret = Some(client_secret);
    config.access_token = Some(tokens.access_token.clone());
    config.refresh_token = Some(tokens.refresh_token);
    config.expires_at = Some(now + tokens.expires_in as i64);
    save_config(&config).map_err(|e| e.to_string())?;
    let body = box_api_get(&tokens.access_token, "https://api.box.com/2.0/users/me")
        .await
        .map_err(|e| e.to_string())?;
    let user: BoxUser = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(format!("{} ({}) としてログインしました", user.name, user.login))
}

#[tauri::command]
async fn developer_token_login(token: String) -> Result<String, String> {
    let body = box_api_get(&token, "https://api.box.com/2.0/users/me")
        .await
        .map_err(|e| e.to_string())?;
    let user: BoxUser = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let mut config = load_config();
    config.developer_token = Some(token);
    save_config(&config).map_err(|e| e.to_string())?;
    Ok(format!("{} ({}) としてログインしました（デベロッパートークン）", user.name, user.login))
}

#[tauri::command]
async fn box_oauth_status() -> Result<OAuthStatus, String> {
    let config = load_config();
    Ok(OAuthStatus {
        logged_in: config.access_token.is_some() || config.developer_token.is_some(),
        expires_at: config.expires_at,
    })
}

#[tauri::command]
async fn box_oauth_logout() -> Result<(), String> {
    let mut config = load_config();
    config.access_token = None;
    config.refresh_token = None;
    config.expires_at = None;
    save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    opener::open_browser(&url).map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_saved_config,
            save_config_cmd,
            process_photos,
            box_api_chat,
            box_api_get_cmd,
            mcp_list_tools,
            mcp_call_tool,
            box_oauth_login,
            developer_token_login,
            box_oauth_status,
            box_oauth_logout,
            open_url,
            copy_to_clipboard
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
