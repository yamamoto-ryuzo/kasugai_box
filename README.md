# Box Photo Geo URL

Boxのフォルダ内の画像ファイルを取得し、EXIF情報から緯度経度・撮影日を抽出し、共有リンク付きで一覧化・GeoJSON/CSV出力する **Tauri + Rust** デスクトップアプリです。

注意: 画像のEXIF情報は画像ファイル自体に埋め込まれているため、画像本体の取得（ダウンロード）が必要です。Box APIは画像のEXIFをメタデータとして返しません。

## 特徴

- Box APIで指定フォルダ（サブフォルダ含む）内の画像ファイルを再帰的に取得
- 画像のEXIF情報から緯度・経度・撮影日を抽出（JPEG/TIFF/HEIC対応）
- 画像ごとにBoxの共有リンク（Web URL）を付与
- 結果をCSVおよびGeoJSON（QGIS対応）で出力
- QGISで「url」列を使ってアクションやHTMLポップアップで写真表示が可能
- 設定（開発者トークン・フォルダURL）は自動保存

---

## 🚀 利用者の方へ

### 1. ダウンロード / インストール

`src-tauri/target/release/bundle/` に MSI インストーラーまたは NSIS インストーラーが生成されています。好きな方を使ってインストールしてください。

- `Box Photo Geo URL_*.msi`
- `Box Photo Geo URL_*-setup.exe`

### 2. 使い方

1. アプリを起動します。
2. 画面に従って Box の `開発者トークン` と `対象フォルダURL` を入力してください。
3. `実行` ボタンを押すと、指定フォルダ以下の画像ファイルを再帰的に取得し、EXIF情報から位置情報・撮影日を抽出します。
4. 処理が完了すると、CSVとGeoJSONが出力されます。デフォルトでは `ドキュメント/box_photo_geo_url/output/` に保存されます。

### 3. 出力ファイル

出力先には以下のファイルが作成されます。

- `box_photos.csv`  
  画像ファイル名、パス、緯度、経度、撮影日、Box共有URLを含むCSV
- `box_photos.geojson`  
  QGIS等で利用できるGeoJSONファイル（EPSG:4326、url列付き）

### 4. QGISでの利用例

- 「url」列をQGISの「アクション」や「HTMLポップアップ」で利用し、写真をWeb表示できます。
  - 例: アクションに `[% "url" %]` を設定しWebブラウザで画像を開く
  - 例: HTMLポップアップに `<img src="[% "url" %]" width="400">` など

---

## 💻 開発者向け情報

### 簡易起動スクリプト

```sh
# 開発モード
cargo tauri dev

# または Python ランチャーから
cd C:\devin\boxPhotoGeoURL
python run.py

# リリースビルド
python run.py -B

# リリース版を起動
python run.py --release
```

### リリースビルド

```sh
cargo tauri build
```

ビルドが完了すると、`src-tauri/target/release/` に実行ファイル、`src-tauri/target/release/bundle/` にインストーラーが生成されます。

---

## ⚠️ 注意事項

- 画像のEXIF情報は画像ファイル自体に埋め込まれているため、**画像をダウンロードせずにEXIF情報を取得することはできません**。
- Box APIは画像のメタデータとしてEXIF情報を直接返すエンドポイントを提供しません。
- EXIFのGPS情報はWGS84（EPSG:4326）を前提としています。
- `access_token` の有効期限切れ時は再取得が必要です。

## 🔗 参考

- [Tauri ドキュメント](https://tauri.app/)
- [Box API ドキュメント](https://developer.box.com/guides/authentication/oauth2/)
- [QGIS アクション/HTMLポップアップ](https://docs.qgis.org/ja/docs/user_manual/working_with_vector/actions.html)
