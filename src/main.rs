use futures::future::BoxFuture;
use futures::FutureExt;
use inquire::Text;
use exif::{In, Reader, Tag, Value};
use regex::Regex;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct Config {
    access_token: Option<String>,
    folder_id: Option<String>,
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

#[derive(Serialize)]
struct PhotoRecord {
    name: String,
    full_name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    date_taken: Option<String>,
    url: String,
}

fn load_config() -> Config {
    if let Ok(content) = fs::read_to_string("config.json") {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

fn save_config(config: &Config) {
    if let Ok(content) = serde_json::to_string_pretty(config) {
        let _ = fs::write("config.json", content);
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
) -> BoxFuture<'a, Result<Vec<(BoxItem, String)>, Box<dyn Error>>> {
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
                return Err(format!("Box API Error {}: {}", status, text).into());
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
                    let mut sub_files = get_image_files_recursive(client, &item.id, &current_path).await?;
                    image_files.append(&mut sub_files);
                }
            }

            if count < limit || data.offset.unwrap_or(0) + count >= data.total_count.unwrap_or(0) {
                break;
            }
            offset += limit;
        }
        Ok(image_files)
    }
    .boxed()
}

fn create_geojson(file_path: &str, records: &[PhotoRecord]) -> Result<(), Box<dyn Error>> {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut config = load_config();

    let access_token = Text::new("開発者トークン:")
        .with_default(config.access_token.as_deref().unwrap_or(""))
        .prompt()?;

    let folder_url = Text::new("フォルダURL:")
        .with_default(config.folder_id.as_deref().unwrap_or(""))
        .prompt()?;

    let folder_id = extract_folder_id(&folder_url);

    config.access_token = Some(access_token.clone());
    config.folder_id = Some(folder_url);
    save_config(&config);

    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", access_token))?,
    );

    let client = Client::builder().default_headers(headers).build()?;

    println!("画像の検索を開始します...");
    let image_files = get_image_files_recursive(&client, &folder_id, "").await?;
    println!("{}件の画像ファイルが見つかりました。解析中...", image_files.len());

    let mut result = Vec::new();

    for (file, parent_path) in image_files {
        let content_url = format!("https://api.box.com/2.0/files/{}/content", file.id);
        
        // Exif解析用なのでエラーが出てもスキップ
        let bytes = match client.get(&content_url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => b,
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

        println!("{}, {:?}, {:?}, {:?}, {}", full_name, lat, lon, date_taken, url);

        result.push(PhotoRecord {
            name: file.name,
            full_name,
            latitude: lat,
            longitude: lon,
            date_taken,
            url,
        });
    }

    if !result.is_empty() {
        // CSV出力
        let mut wtr = csv::Writer::from_path("box_photos.csv")?;
        for r in &result {
            wtr.serialize(r)?;
        }
        wtr.flush()?;
        println!("CSVファイル(box_photos.csv)を作成しました。");

        // GPKG出力
        let has_geom = result.iter().any(|r| r.latitude.is_some() && r.longitude.is_some());
        if has_geom {
            create_geojson("box_photos.geojson", &result)?;
            println!("GeoJSONファイル(box_photos.geojson)を作成しました。");
            println!("QGISにドラッグ＆ドロップするだけで、写真の場所を表示できます。");
        } else {
            println!("位置情報付き画像がありません。");
        }
    } else {
        println!("対象ファイルが見つかりませんでした。");
    }

    // ユーザーがEnterを押すまで待機する（GUIアプリからの移行向け）
    println!("\n処理が完了しました。Enterキーを押して終了してください...");
    let _ = Text::new("").prompt();

    Ok(())
}
