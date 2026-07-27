use anyhow::{Context, Result};
use futures::future::BoxFuture;
use futures::FutureExt;
use regex::Regex;
use reqwest::{header, Client};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::box_api::{fetch_embedded_metadata, BoxFolderItems, BoxItem};

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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub records: Vec<PhotoRecord>,
    pub csv_path: String,
    pub geojson_path: Option<String>,
    pub output_dir: String,
    pub message: String,
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

fn parse_coordinate(s: &str) -> Option<f64> {
    let dms_re = Regex::new(r#"(\d+(?:\.\d+)?)\s*deg\s*(\d+(?:\.\d+)?)\s*['′]\s*([\d.]+)\s*(?:"|''|″)?\s*([NSEW])"#).unwrap();
    if let Some(caps) = dms_re.captures(s) {
        let d: f64 = caps[1].parse().ok()?;
        let m: f64 = caps[2].parse().ok()?;
        let sec: f64 = caps[3].parse().ok()?;
        let mut deg = d + m / 60.0 + sec / 3600.0;
        if &caps[4] == "S" || &caps[4] == "W" {
            deg = -deg;
        }
        return Some(deg);
    }

    let dec_re = Regex::new(r"^([+-]?\d+(?:\.\d+)?)(?:\s*([NSEW]))?$").unwrap();
    if let Some(caps) = dec_re.captures(s.trim()) {
        let mut v: f64 = caps[1].parse().ok()?;
        if let Some(h) = caps.get(2).map(|m| m.as_str()) {
            if h == "S" || h == "W" {
                v = -v.abs();
            }
        }
        return Some(v);
    }

    None
}

fn extract_embedded_location_and_datetime(
    metadata: &Value,
) -> (Option<f64>, Option<f64>, Option<String>) {
    let fallback = Value::Null;
    let root = metadata
        .as_array()
        .and_then(|a| a.first())
        .unwrap_or(&fallback);

    let mut lat = None;
    let mut lon = None;

    if let Some(root_obj) = root.as_object() {
        'outer: for group in ["Composite", "EXIF", "BoxNormalized"] {
            if let Some(obj) = root_obj.get(group).and_then(Value::as_object) {
                if let Some(pos) = obj.get("GPSPosition").and_then(Value::as_str) {
                    let parts: Vec<&str> = pos.split(',').collect();
                    if parts.len() == 2 {
                        if let (Some(la), Some(lo)) =
                            (parse_coordinate(parts[0]), parse_coordinate(parts[1]))
                        {
                            lat = Some(la);
                            lon = Some(lo);
                            break 'outer;
                        }
                    }
                }

                if let (Some(la), Some(lo)) = (
                    obj.get("GPSLatitude").and_then(Value::as_str),
                    obj.get("GPSLongitude").and_then(Value::as_str),
                ) {
                    if let (Some(mut la), Some(mut lo)) =
                        (parse_coordinate(la), parse_coordinate(lo))
                    {
                        if group == "EXIF" {
                            if let Some(lat_ref) =
                                obj.get("GPSLatitudeRef").and_then(Value::as_str)
                            {
                                if lat_ref == "S" {
                                    la = -la.abs();
                                }
                            }
                            if let Some(lon_ref) =
                                obj.get("GPSLongitudeRef").and_then(Value::as_str)
                            {
                                if lon_ref == "W" {
                                    lo = -lo.abs();
                                }
                            }
                        }
                        lat = Some(la);
                        lon = Some(lo);
                        break 'outer;
                    }
                }
            }
        }
    }

    let date_taken = root.as_object().and_then(|root_obj| {
        for group in ["EXIF", "Composite"] {
            if let Some(obj) = root_obj.get(group).and_then(Value::as_object) {
                for key in ["DateTimeOriginal", "CreateDate", "DateTime", "DateTimeDigitized"] {
                    if let Some(v) = obj.get(key).and_then(Value::as_str) {
                        return Some(v.to_string());
                    }
                }
            }
        }
        None
    });

    (lat, lon, date_taken)
}

fn get_image_files_recursive<'a, F>(
    client: &'a Client,
    folder_id: &'a str,
    parent_path: &'a str,
    progress: F,
) -> BoxFuture<'a, Result<Vec<(BoxItem, String)>>>
where
    F: Fn(u8, String) -> bool + Copy + Send + 'a,
{
    async move {
        let display_path = if parent_path.is_empty() { "ルート" } else { parent_path };
        if !progress(1, format!("フォルダを取得中: {}", display_path)) {
            return Err(anyhow::anyhow!("処理を停止しました"));
        }

        let mut image_files = Vec::new();
        let mut offset = 0;
        let limit = 1000;

        loop {
            if !progress(1, format!("{}件の画像を発見（{}）", image_files.len(), display_path)) {
                return Err(anyhow::anyhow!("処理を停止しました"));
            }

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
                        get_image_files_recursive(client, &item.id, &current_path, progress).await?;
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

pub async fn run_process(
    token: String,
    folder_urls: Vec<String>,
    output_dir: String,
    progress: impl Fn(u8, String) -> bool + Copy + Send,
) -> Result<ProcessResult> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", token))?,
    );

    let client = Client::builder().default_headers(headers).build()?;

    let folder_count = folder_urls.len();
    let mut image_files = Vec::new();
    for (i, folder_url) in folder_urls.iter().enumerate() {
        if !progress(1, format!("{} / {} フォルダをスキャン中...", i + 1, folder_count)) {
            return Err(anyhow::anyhow!("処理を停止しました"));
        }
        let folder_id = extract_folder_id(folder_url);
        let mut files = get_image_files_recursive(&client, &folder_id, "", progress).await?;
        image_files.append(&mut files);
    }

    let total = image_files.len();
    if !progress(
        5,
        format!("{}件の画像ファイルが見つかりました。メタデータを取得中...", total),
    ) {
        return Err(anyhow::anyhow!("処理を停止しました"));
    }

    let mut records = Vec::new();

    for (index, (file, parent_path)) in image_files.into_iter().enumerate() {
        let (lat, lon, date_taken) = match fetch_embedded_metadata(&token, &file.id).await {
            Ok(meta) => extract_embedded_location_and_datetime(&meta),
            Err(_) => (None, None, None),
        };

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

        if total > 0 {
            let p = 5 + ((index + 1) * 90 / total) as u8;
            if !progress(p, format!("{} / {} 件の画像を処理中...", index + 1, total)) {
                return Err(anyhow::anyhow!("処理を停止しました"));
            }
        }
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

    if !progress(100, "処理を完了しました".into()) {
        return Err(anyhow::anyhow!("処理を停止しました"));
    }

    Ok(ProcessResult {
        records,
        csv_path: csv_path.to_string_lossy().to_string(),
        geojson_path,
        output_dir: output.to_string_lossy().to_string(),
        message,
    })
}
