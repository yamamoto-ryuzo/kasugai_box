use anyhow::{Context, Result};
use exif::{In, Reader, Tag, Value};
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
    pub access_token: Option<String>,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub records: Vec<PhotoRecord>,
    pub csv_path: String,
    pub geojson_path: Option<String>,
    pub message: String,
}

fn app_config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("box_photo_geo_url_rs/config.json")
}

fn load_config() -> Config {
    let path = app_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

fn save_config(config: &Config) -> Result<()> {
    let path = app_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    fs::write(&path, content)?;
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
    save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn process_photos(
    token: String,
    folder_url: String,
    output_dir: String,
) -> Result<ProcessResult, String> {
    run_process(token, folder_url, output_dir)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_saved_config,
            save_config_cmd,
            process_photos
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
