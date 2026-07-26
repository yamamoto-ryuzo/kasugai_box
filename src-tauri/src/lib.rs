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

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub box_subject_type: Option<String>,
    pub box_subject_id: Option<String>,
    pub folder_url: Option<String>,
}

#[derive(Deserialize, Debug)]
struct BoxItem {
    r#type: String,
    id: String,
    name: String,
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
struct BoxTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
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
}

async fn get_box_access_token(
    client_id: &str,
    client_secret: &str,
    subject_type: &str,
    subject_id: &str,
) -> Result<String> {
    let mut params = std::collections::HashMap::new();
    params.insert("grant_type", "client_credentials");
    params.insert("client_id", client_id);
    params.insert("client_secret", client_secret);
    params.insert("box_subject_type", subject_type);
    params.insert("box_subject_id", subject_id);

    let resp = Client::new()
        .post("https://api.box.com/oauth2/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Box token error {}: {}", status, text));
    }

    let data: BoxTokenResponse = resp.json().await?;
    Ok(data.access_token)
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
    client_id: String,
    client_secret: String,
    box_subject_type: String,
    box_subject_id: String,
    folder_url: String,
    output_dir: String,
) -> Result<ProcessResult> {
    let token = get_box_access_token(
        &client_id,
        &client_secret,
        &box_subject_type,
        &box_subject_id,
    )
    .await?;

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
    save_config(&config).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpChatResponse {
    pub reply: String,
}

async fn box_api_get(token: &str, url: &str) -> Result<String> {
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

async fn run_mcp_chat(text: String) -> Result<McpChatResponse> {
    let config = load_config();
    let client_id = config.client_id.context("クライアントIDが設定されていません")?;
    let client_secret = config.client_secret.context("クライアントシークレットが設定されていません")?;
    let subject_type = config.box_subject_type.context("Subject Typeが設定されていません")?;
    let subject_id = config.box_subject_id.context("Subject IDが設定されていません")?;

    let token = get_box_access_token(&client_id, &client_secret, &subject_type, &subject_id).await?;
    let parts: Vec<&str> = text.trim().splitn(2, ' ').collect();
    let cmd = parts.first().copied().unwrap_or("").to_lowercase();
    let arg = parts.get(1).copied().unwrap_or("").trim();

    let reply = match cmd.as_str() {
        "help" | "?" | "" => {
            "利用可能なコマンド:\n".to_string()
                + "  help        このヘルプを表示\n"
                + "  me          ログインユーザー情報を表示\n"
                + "  folder <id> フォルダ内のアイテムを一覧\n"
                + "  search <q>  ファイルを検索"
        }
        "me" | "user" => {
            let body = box_api_get(&token, "https://api.box.com/2.0/users/me").await?;
            let user: BoxUser = serde_json::from_str(&body)?;
            format!("ユーザー: {} ({} / {})", user.name, user.login, user.r#type)
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
                "アイテムが見つかりませんでした。".to_string()
            } else {
                data.entries
                    .iter()
                    .map(|i| format!("[{}] {} ({})", i.r#type, i.name, i.id))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        "search" => {
            if arg.is_empty() {
                "検索語を入力してください。".to_string()
            } else {
                let url = format!(
                    "https://api.box.com/2.0/search?query={}&limit=20&fields=type,id,name",
                    arg
                );
                let body = box_api_get(&token, &url).await?;
                let data: BoxSearchResponse = serde_json::from_str(&body)?;
                if data.entries.is_empty() {
                    "検索結果が見つかりませんでした。".to_string()
                } else {
                    data.entries
                        .iter()
                        .map(|i| format!("[{}] {} ({})", i.r#type, i.name, i.id))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
        }
        _ => "不明なコマンドです。'help' で使い方を確認できます。".to_string(),
    };

    Ok(McpChatResponse { reply })
}

#[tauri::command]
async fn mcp_chat(text: String) -> Result<McpChatResponse, String> {
    run_mcp_chat(text).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn process_photos(
    client_id: String,
    client_secret: String,
    box_subject_type: String,
    box_subject_id: String,
    folder_url: String,
    output_dir: String,
) -> Result<ProcessResult, String> {
    run_process(
        client_id,
        client_secret,
        box_subject_type,
        box_subject_id,
        folder_url,
        output_dir,
    )
    .await
    .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_saved_config,
            save_config_cmd,
            process_photos,
            mcp_chat
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
