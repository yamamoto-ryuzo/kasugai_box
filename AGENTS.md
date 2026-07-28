# 開発メモ

## 検証コマンド

サーバー（Rust / `server/`）:

```powershell
cd server
cargo build
cargo test --bin kasugai_box
```

Box の実 API に対する疎通確認（既定では `#[ignore]`）:

```powershell
cd server
$env:BOX_TOKEN="<Box 開発者トークン>"
$env:BOX_FOLDER_ID="<フォルダID または 0>"
cargo test --bin kasugai_box -- --ignored --nocapture live_run_process
```

## Box 埋め込みメタデータ（写真の位置情報）

`server/src/box_api.rs` の `fetch_embedded_metadata` は
`GET /2.0/files/:id?fields=representations` に `x-rep-hints: [embedded_metadata]`
を付けて ExifTool 形式の JSON を取得する。ondemand 生成のため `state` が `none`
のときは `info.url` を GET してトリガーし、`success` / `viewable` になるまでポーリングする。

実レスポンスで確認済みの注意点:

- グループは `File` / `EXIF` / `XMP` / `JFIF` / `Photoshop` / `BoxNormalized` など。
  ExifTool の `Composite` グループは**返ってこない**ため `Composite.GPSPosition` に依存できない。
- GPS 値は `EXIF` グループに PrintConv 済みの**文字列**で入る
  （例: `GPSLatitude: "35 deg 39' 29.99\""`）。方位記号は値に含まれず、
  `GPSLatitudeRef` に `"North"` / `"South"` として格納される（`"N"` / `"S"` ではない）。
- GPS 情報を持たない写真もタグ自体は存在し、`""` / `"undef"` / `"Unknown ()"` を返す。
- 撮影日時は `EXIF.DateTimeOriginal` に `"2026:07:26 14:58:02"` 形式で入る。
