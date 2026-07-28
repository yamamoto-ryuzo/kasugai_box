use anyhow::{Context, Result};
use futures::future::BoxFuture;
use futures::stream::{self, StreamExt};
use futures::FutureExt;
use regex::Regex;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::box_api::{fetch_embedded_metadata, BoxFolderItems, BoxItem, BoxPathCollection};

/// embedded_metadata 取得の同時実行数。Box のレート制限に配慮した値。
const METADATA_CONCURRENCY: usize = 8;

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

async fn fetch_folder_prefix(client: &Client, folder_id: &str) -> Result<String> {
    let url = format!(
        "https://api.box.com/2.0/folders/{}?fields=name,path_collection",
        folder_id
    );
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Box API Error {}: {}", status, text));
    }
    #[derive(Deserialize)]
    struct FolderInfo {
        name: String,
        path_collection: BoxPathCollection,
    }
    let info: FolderInfo = resp.json().await?;
    let mut parts: Vec<&str> = info
        .path_collection
        .entries
        .iter()
        .skip(1)
        .map(|e| e.name.as_str())
        .collect();
    parts.push(&info.name);
    Ok(parts.join("/"))
}

/// ExifTool が値を持たないときに出力するプレースホルダ。
/// Box の embedded_metadata では GPS 情報のない写真が `""` / `"undef"` / `"Unknown ()"` を返す。
fn is_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.is_empty()
        || t.eq_ignore_ascii_case("undef")
        || t.starts_with("Unknown")
        || t.starts_with("(Binary data")
}

/// `N`/`S`/`E`/`W` および ExifTool PrintConv の `North`/`South`/`East`/`West` を符号に変換する。
fn hemisphere_sign(s: &str) -> Option<f64> {
    let t = s.trim();
    if is_placeholder(t) {
        return None;
    }
    match t.chars().next()?.to_ascii_uppercase() {
        'N' | 'E' => Some(1.0),
        'S' | 'W' => Some(-1.0),
        _ => None,
    }
}

/// 座標文字列/数値を10進度に変換する。戻り値の `bool` は値自身に方位が含まれていたかを表す。
///
/// Box の embedded_metadata は ExifTool の PrintConv 済み文字列を返すため、複数の形式に対応する:
/// - `EXIF` グループ: `35 deg 39' 29.99"`（方位は `GPSLatitudeRef` に分離）
/// - `Composite` グループ: `35 deg 39' 29.99" N`
/// - `XMP` グループ: `35,39.4999N`
/// - 10進数（文字列・JSON 数値の両方）: `35.658`, `N 35.658`
fn parse_coordinate(value: &Value) -> Option<(f64, bool)> {
    if let Some(n) = value.as_f64() {
        return Some((n, false));
    }
    let raw = value.as_str()?;
    if is_placeholder(raw) {
        return None;
    }

    // `deg` の `e` が方位 `E` と衝突するため先に除去する。
    let cleaned = Regex::new(r"(?i)deg").unwrap().replace_all(raw, " ");

    // 方位を検出して除去する。単語形（North）→ 末尾の記号（`35,39.4999N`）→ 先頭の記号（`N 35.658`）の順。
    let patterns = [
        r"(?i)\b(north|south|east|west)\b",
        r"(?i)([nsew])\s*$",
        r"(?i)^\s*([nsew])\b",
    ];
    let mut sign = None;
    let mut body = cleaned.to_string();
    for pattern in patterns {
        let re = Regex::new(pattern).unwrap();
        if let Some(caps) = re.captures(&body) {
            sign = hemisphere_sign(caps.get(1)?.as_str());
            body = re.replace(&body, " ").to_string();
            break;
        }
    }

    // 残った数値を D / D M / D M S として解釈する。
    let num_re = Regex::new(r"[+-]?\d+(?:\.\d+)?").unwrap();
    let nums: Vec<f64> = num_re
        .find_iter(&body)
        .filter_map(|m| m.as_str().parse::<f64>().ok())
        .collect();
    let magnitude = match nums.as_slice() {
        [d] => *d,
        [d, m] => d.abs() + m / 60.0,
        [d, m, s, ..] => d.abs() + m / 60.0 + s / 3600.0,
        [] => return None,
    };

    match sign {
        Some(sign) => Some((sign * magnitude.abs(), true)),
        None => Some((magnitude, false)),
    }
}

/// グループ内の座標を、必要なら `*Ref` キーの方位を適用して取得する。
fn coordinate_from_group(
    group: &serde_json::Map<String, Value>,
    key: &str,
    ref_key: &str,
) -> Option<f64> {
    let (value, has_hemisphere) = parse_coordinate(group.get(key)?)?;
    if has_hemisphere {
        return Some(value);
    }
    match group.get(ref_key).and_then(Value::as_str).and_then(hemisphere_sign) {
        Some(sign) => Some(sign * value.abs()),
        None => Some(value),
    }
}

/// ExifTool のグループを探索順に並べる。Box は `Composite` を返さないため
/// `EXIF` / `XMP` を主軸に、未知のグループも最後に走査する。
fn metadata_groups(root: &Value) -> Vec<&serde_json::Map<String, Value>> {
    const PREFERRED: [&str; 4] = ["Composite", "EXIF", "XMP", "GPS"];
    let Some(root_obj) = root.as_object() else {
        return Vec::new();
    };
    let mut groups = vec![root_obj];
    for name in PREFERRED {
        if let Some(g) = root_obj.get(name).and_then(Value::as_object) {
            groups.push(g);
        }
    }
    for (name, value) in root_obj {
        if !PREFERRED.contains(&name.as_str()) {
            if let Some(g) = value.as_object() {
                groups.push(g);
            }
        }
    }
    groups
}

fn extract_embedded_location_and_datetime(
    metadata: &Value,
) -> (Option<f64>, Option<f64>, Option<String>) {
    let fallback = Value::Null;
    let root = metadata
        .as_array()
        .and_then(|a| a.first())
        .unwrap_or(metadata)
        .clone();
    let root = if root.is_null() { fallback } else { root };

    let groups = metadata_groups(&root);

    let mut lat = None;
    let mut lon = None;
    for group in &groups {
        // `Composite.GPSPosition` は "緯度, 経度" の1フィールドにまとまっている。
        if let Some(pos) = group.get("GPSPosition").and_then(Value::as_str) {
            if !is_placeholder(pos) {
                let parts: Vec<&str> = pos.split(',').collect();
                if parts.len() == 2 {
                    if let (Some((la, _)), Some((lo, _))) = (
                        parse_coordinate(&Value::from(parts[0])),
                        parse_coordinate(&Value::from(parts[1])),
                    ) {
                        lat = Some(la);
                        lon = Some(lo);
                        break;
                    }
                }
            }
        }

        if let (Some(la), Some(lo)) = (
            coordinate_from_group(group, "GPSLatitude", "GPSLatitudeRef"),
            coordinate_from_group(group, "GPSLongitude", "GPSLongitudeRef"),
        ) {
            // 0,0 は「値なし」を意味する破損 EXIF が多いため採用しない。
            if la != 0.0 || lo != 0.0 {
                lat = Some(la);
                lon = Some(lo);
                break;
            }
        }
    }

    let mut date_taken = None;
    'date: for key in [
        "DateTimeOriginal",
        "CreateDate",
        "DateTimeDigitized",
        "DateTime",
        "ModifyDate",
    ] {
        for group in &groups {
            if let Some(v) = group.get(key).and_then(Value::as_str) {
                if !is_placeholder(v) {
                    date_taken = Some(v.to_string());
                    break 'date;
                }
            }
        }
    }

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
        let prefix = fetch_folder_prefix(&client, &folder_id).await?;
        let mut files = get_image_files_recursive(&client, &folder_id, &prefix, progress).await?;
        image_files.append(&mut files);
    }

    let total = image_files.len();
    if !progress(
        5,
        format!("{}件の画像ファイルが見つかりました。メタデータを取得中...", total),
    ) {
        return Err(anyhow::anyhow!("処理を停止しました"));
    }

    // メタデータ取得は representation の生成待ちを含むため、並列度を上げて実行する。
    let mut slots: Vec<Option<PhotoRecord>> = vec![None; total];
    let mut errors: Vec<String> = Vec::new();
    let mut done = 0usize;

    {
        let client_ref = &client;
        let mut stream = stream::iter(image_files.into_iter().enumerate().map(
            |(index, (file, parent_path))| async move {
                let meta = fetch_embedded_metadata(client_ref, &file.id).await;
                (index, file, parent_path, meta)
            },
        ))
        .buffer_unordered(METADATA_CONCURRENCY);

        while let Some((index, file, parent_path, meta)) = stream.next().await {
            let (lat, lon, date_taken) = match meta {
                Ok(meta) => extract_embedded_location_and_datetime(&meta),
                Err(e) => {
                    errors.push(format!("{}: {}", file.name, e));
                    (None, None, None)
                }
            };

            let url = format!("https://app.box.com/file/{}", file.id);
            let full_name = if parent_path.is_empty() {
                file.name.clone()
            } else {
                format!("{}/{}", parent_path, file.name)
            };

            slots[index] = Some(PhotoRecord {
                name: file.name,
                full_name,
                latitude: lat,
                longitude: lon,
                date_taken,
                url,
            });

            done += 1;
            let p = 5 + (done * 90 / total.max(1)) as u8;
            if !progress(p, format!("{} / {} 件の画像を処理中...", done, total)) {
                return Err(anyhow::anyhow!("処理を停止しました"));
            }
        }
    }

    for message in errors.iter().take(10) {
        eprintln!("[photos] メタデータ取得に失敗: {}", message);
    }
    let records: Vec<PhotoRecord> = slots.into_iter().flatten().collect();

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

    let located = records
        .iter()
        .filter(|r| r.latitude.is_some() && r.longitude.is_some())
        .count();
    let mut message = if records.is_empty() {
        "対象ファイルが見つかりませんでした。".to_string()
    } else if geojson_path.is_some() {
        format!(
            "{}件の画像を処理しました（位置情報あり {}件）。CSVとGeoJSONを出力しました。",
            records.len(),
            located
        )
    } else {
        format!("{}件の画像を処理しました。位置情報が含まれていませんでした。", records.len())
    };
    if let Some(first) = errors.first() {
        message.push_str(&format!(
            " {}件のメタデータ取得に失敗しました（例: {}）。",
            errors.len(),
            first
        ));
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn approx(a: Option<f64>, b: f64) {
        let a = a.expect("値がありません");
        assert!((a - b).abs() < 1e-6, "{} != {}", a, b);
    }

    #[test]
    fn parses_exiftool_dms_without_hemisphere() {
        let (v, has_hemi) = parse_coordinate(&json!("35 deg 39' 29.99\"")).unwrap();
        assert!(!has_hemi);
        assert!((v - 35.658330).abs() < 1e-5);
    }

    #[test]
    fn parses_dms_with_hemisphere() {
        let (v, has_hemi) = parse_coordinate(&json!("139 deg 44' 28.80\" W")).unwrap();
        assert!(has_hemi);
        assert!((v + 139.741333).abs() < 1e-5);
    }

    #[test]
    fn parses_decimal_and_numeric_values() {
        assert!(!parse_coordinate(&json!(35.6583)).unwrap().1);
        approx(Some(parse_coordinate(&json!(35.6583)).unwrap().0), 35.6583);
        approx(Some(parse_coordinate(&json!("-35.6583")).unwrap().0), -35.6583);
        approx(Some(parse_coordinate(&json!("35.6583 S")).unwrap().0), -35.6583);
    }

    #[test]
    fn parses_xmp_degrees_minutes() {
        let (v, has_hemi) = parse_coordinate(&json!("35,39.4999N")).unwrap();
        assert!(has_hemi);
        assert!((v - 35.658331).abs() < 1e-4);
    }

    #[test]
    fn rejects_exiftool_placeholders() {
        for raw in ["", "   ", "undef", "Unknown ()", "(Binary data 1024 bytes)"] {
            assert!(parse_coordinate(&json!(raw)).is_none(), "{} を拒否できていません", raw);
        }
    }

    /// Box の実レスポンス形式: GPS は `EXIF` グループに PrintConv 済み文字列、
    /// 方位は `GPSLatitudeRef` に `North`/`South` として格納される。`Composite` は存在しない。
    #[test]
    fn extracts_location_from_box_exif_group() {
        let meta = json!([{
            "BoxNormalized": { "PageCount": null },
            "EXIF": {
                "DateTimeOriginal": "2026:07:26 14:58:02",
                "GPSLatitude": "35 deg 39' 29.99\"",
                "GPSLatitudeRef": "North",
                "GPSLongitude": "139 deg 44' 28.80\"",
                "GPSLongitudeRef": "East",
                "GPSAltitude": "12.5 m"
            },
            "File": { "FileType": "JPEG" }
        }]);
        let (lat, lon, date) = extract_embedded_location_and_datetime(&meta);
        approx(lat, 35.658330);
        approx(lon, 139.741333);
        assert_eq!(date.as_deref(), Some("2026:07:26 14:58:02"));
    }

    #[test]
    fn applies_southern_and_western_refs() {
        let meta = json!([{
            "EXIF": {
                "GPSLatitude": "33 deg 51' 54.00\"",
                "GPSLatitudeRef": "South",
                "GPSLongitude": "151 deg 12' 36.00\"",
                "GPSLongitudeRef": "West"
            }
        }]);
        let (lat, lon, _) = extract_embedded_location_and_datetime(&meta);
        approx(lat, -33.865);
        approx(lon, -151.21);
    }

    /// GPS タグはあるが値が空の写真（実データで確認済み）は位置情報なしとして扱う。
    #[test]
    fn treats_empty_gps_tags_as_missing() {
        let meta = json!([{
            "EXIF": {
                "DateTimeOriginal": "2026:07:26 14:58:02",
                "GPSAltitude": "undef",
                "GPSLatitude": "",
                "GPSLatitudeRef": "Unknown ()",
                "GPSLongitude": "",
                "GPSLongitudeRef": "Unknown ()"
            }
        }]);
        let (lat, lon, date) = extract_embedded_location_and_datetime(&meta);
        assert!(lat.is_none() && lon.is_none());
        assert_eq!(date.as_deref(), Some("2026:07:26 14:58:02"));
    }

    #[test]
    fn supports_composite_gps_position() {
        let meta = json!([{
            "Composite": { "GPSPosition": "35 deg 39' 29.99\" N, 139 deg 44' 28.80\" E" }
        }]);
        let (lat, lon, _) = extract_embedded_location_and_datetime(&meta);
        approx(lat, 35.658330);
        approx(lon, 139.741333);
    }

    /// 実 Box API に対する疎通確認。既定では無視される。
    /// 実行例: `BOX_TOKEN=xxx BOX_FOLDER_ID=0 cargo test --bin kasugai_box -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_run_process_smoke() {
        let token = std::env::var("BOX_TOKEN").expect("BOX_TOKEN が必要です");
        let folder = std::env::var("BOX_FOLDER_ID").unwrap_or_else(|_| "0".to_string());
        let out = std::env::temp_dir().join("kasugai_box_photo_test");
        let result = run_process(token, vec![folder], out.to_string_lossy().to_string(), |p, m| {
            println!("[{}%] {}", p, m);
            true
        })
        .await
        .expect("run_process が失敗しました");
        println!("message: {}", result.message);
        for r in result.records.iter().take(20) {
            println!(
                "{} lat={:?} lon={:?} date={:?}",
                r.full_name, r.latitude, r.longitude, r.date_taken
            );
        }
    }

    #[test]
    fn ignores_zero_zero_coordinates() {
        let meta = json!([{
            "EXIF": { "GPSLatitude": "0 deg 0' 0.00\"", "GPSLongitude": "0 deg 0' 0.00\"" }
        }]);
        let (lat, lon, _) = extract_embedded_location_and_datetime(&meta);
        assert!(lat.is_none() && lon.is_none());
    }
}
