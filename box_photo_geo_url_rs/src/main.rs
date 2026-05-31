use futures::future::BoxFuture;
use futures::FutureExt;
use inquire::Text;
use exif::{In, Reader, Tag, Value};
use regex::Regex;
use reqwest::{header, Client};
use rusqlite::Connection;
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

fn create_gpkg(db_path: &str, records: &[PhotoRecord]) -> rusqlite::Result<()> {
    if Path::new(db_path).exists() {
        fs::remove_file(db_path).unwrap_or_default();
    }
    let conn = Connection::open(db_path)?;
    conn.execute("PRAGMA application_id = 1196444487;", [])?;
    conn.execute("PRAGMA user_version = 10300;", [])?;

    conn.execute(
        "CREATE TABLE gpkg_spatial_ref_sys (
            srs_name TEXT NOT NULL,
            srs_id INTEGER NOT NULL PRIMARY KEY,
            organization TEXT NOT NULL,
            organization_coordsys_id INTEGER NOT NULL,
            definition  TEXT NOT NULL,
            description TEXT
        )",
        [],
    )?;

    conn.execute(
        "INSERT INTO gpkg_spatial_ref_sys 
        (srs_name, srs_id, organization, organization_coordsys_id, definition, description) 
        VALUES ('Undefined Cartesian SRS', -1, 'NONE', -1, 'undefined', 'undefined cartesian coordinate reference system')",
        [],
    )?;

    conn.execute(
        "INSERT INTO gpkg_spatial_ref_sys 
        (srs_name, srs_id, organization, organization_coordsys_id, definition, description) 
        VALUES ('Undefined Geographic SRS', 0, 'NONE', 0, 'undefined', 'undefined geographic coordinate reference system')",
        [],
    )?;

    conn.execute(
        "INSERT INTO gpkg_spatial_ref_sys 
        (srs_name, srs_id, organization, organization_coordsys_id, definition, description) 
        VALUES ('WGS 84', 4326, 'EPSG', 4326, 'GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563,AUTHORITY[\"EPSG\",\"7030\"]],AUTHORITY[\"EPSG\",\"6326\"]],PRIMEM[\"Greenwich\",0,AUTHORITY[\"EPSG\",\"8901\"]],UNIT[\"degree\",0.0174532925199433,AUTHORITY[\"EPSG\",\"9122\"]],AUTHORITY[\"EPSG\",\"4326\"]]', 'WGS 84')",
        [],
    )?;

    conn.execute(
        "CREATE TABLE gpkg_contents (
            table_name TEXT NOT NULL PRIMARY KEY,
            data_type TEXT NOT NULL,
            identifier TEXT UNIQUE,
            description TEXT DEFAULT '',
            last_change DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            min_x DOUBLE, min_y DOUBLE, max_x DOUBLE, max_y DOUBLE,
            srs_id INTEGER
        )",
        [],
    )?;

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut has_bounds = false;

    for r in records {
        if let (Some(lat), Some(lon)) = (r.latitude, r.longitude) {
            min_x = min_x.min(lon);
            max_x = max_x.max(lon);
            min_y = min_y.min(lat);
            max_y = max_y.max(lat);
            has_bounds = true;
        }
    }

    if has_bounds {
        conn.execute(
            "INSERT INTO gpkg_contents (table_name, data_type, identifier, srs_id, min_x, min_y, max_x, max_y) 
            VALUES ('box_photos', 'features', 'box_photos', 4326, ?, ?, ?, ?)",
            rusqlite::params![min_x, min_y, max_x, max_y],
        )?;
    } else {
        conn.execute(
            "INSERT INTO gpkg_contents (table_name, data_type, identifier, srs_id) 
            VALUES ('box_photos', 'features', 'box_photos', 4326)",
            [],
        )?;
    }

    conn.execute(
        "CREATE TABLE gpkg_geometry_columns (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            geometry_type_name TEXT NOT NULL,
            srs_id INTEGER NOT NULL,
            z TINYINT NOT NULL,
            m TINYINT NOT NULL,
            CONSTRAINT pk_geom_cols PRIMARY KEY (table_name, column_name),
            CONSTRAINT uk_gc_table_name UNIQUE (table_name),
            CONSTRAINT fk_gc_tn FOREIGN KEY (table_name) REFERENCES gpkg_contents(table_name),
            CONSTRAINT fk_gc_srs FOREIGN KEY (srs_id) REFERENCES gpkg_spatial_ref_sys (srs_id)
        )",
        [],
    )?;

    conn.execute(
        "INSERT INTO gpkg_geometry_columns (table_name, column_name, geometry_type_name, srs_id, z, m) 
        VALUES ('box_photos', 'geom', 'POINT', 4326, 0, 0)",
        [],
    )?;

    conn.execute(
        "CREATE TABLE box_photos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            geom POINT,
            name TEXT,
            full_name TEXT,
            url TEXT,
            date_taken TEXT
        )",
        [],
    )?;

    let mut stmt = conn.prepare(
        "INSERT INTO box_photos (geom, name, full_name, url, date_taken) 
        VALUES (?, ?, ?, ?, ?)"
    )?;

    for r in records {
        if let (Some(lat), Some(lon)) = (r.latitude, r.longitude) {
            let mut blob = Vec::with_capacity(29);
            blob.extend_from_slice(b"GP");
            blob.push(0);
            blob.push(1);
            blob.extend_from_slice(&4326i32.to_le_bytes());
            blob.push(1);
            blob.extend_from_slice(&1u32.to_le_bytes());
            blob.extend_from_slice(&lon.to_le_bytes());
            blob.extend_from_slice(&lat.to_le_bytes());

            stmt.execute(rusqlite::params![
                blob,
                r.name,
                r.full_name,
                r.url,
                r.date_taken
            ])?;
        }
    }

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
            create_gpkg("box_photos.gpkg", &result)?;
            println!("GPKGファイル(box_photos.gpkg)を作成しました。");
            println!("QGISで「url」列を使ってアクションやHTMLポップアップで写真を表示できます。");
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
