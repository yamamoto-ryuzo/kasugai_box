# kasugai_box

KASUGAI 用 API サイドカーです。Box フォルダ内の画像ファイルを取得し、EXIF 情報から緯度経度・撮影日を抽出し、共有リンク付きで一覧化・GeoJSON/CSV 出力を行います。

KASUGAI 本体は `http://127.0.0.1:8410/ui` を WebView のペイン/タブで読み込みます。AI エージェント向けには `POST /mcp` で同じ機能をツールとして提供します。

> 画像の EXIF 情報は画像ファイル自体に埋め込まれているため、画像本体の取得（ダウンロード）が必要です。Box API は画像の EXIF をメタデータとして返しません。

---

## ドキュメント索引

| ドキュメント | 内容 |
| :--- | :--- |
| [OpenAPI 仕様](./openapi.yaml) | REST API / MCP エンドポイントの完全な仕様 |
| [KASUGAI 外部連携方針](../kasugai/KASUGAI_INTEGRATION_POLICY.md) | KASUGAI サイドカー全体の設計方針 |
| [Changelog](./CHANGELOG.md) | リリース履歴と変更点 |
| [GitHub リポジトリ](https://github.com/yamamoto-ryuzo/kasugai_box) | ソースコード・Issue |

---

## エンドポイント一覧

| 用途 | エンドポイント |
| :--- | :--- |
| ヘルスチェック | `GET /health` |
| UI | `GET /ui` または `/` |
| OpenAPI 仕様 | `GET /openapi.yaml` |
| 設定取得/更新 | `GET/POST /api/v1/config` |
| Box OAuth ログイン | `POST /api/v1/auth/box/login` |
| デベロッパートークン | `POST /api/v1/auth/box/developer-token` |
| ログイン状態 | `GET /api/v1/auth/box/status` |
| ログアウト | `POST /api/v1/auth/box/logout` |
| 写真処理ジョブ開始 | `POST /api/v1/photos/process` |
| ジョブ進捗・結果 | `GET /api/v1/jobs/{id}` |
| Box API チャット | `POST /api/v1/box/chat` |
| 外部 MCP ツール操作 | `GET /api/v1/mcp-client/tools`, `POST /api/v1/mcp-client/call` |
| AI エージェント用 MCP | `POST /mcp` |

---

## クイックスタート

### 1. 起動

```sh
cd C:\devin\kasugai_box\server
cargo run --release
```

既定ポートは `8410` です。設定タブまたは `KASUGAI_BOX_PORT` 環境変数で変更できます。

```powershell
$env:KASUGAI_BOX_PORT = "8411"
```

### 2. KASUGAI からの接続

KASUGAI 側の設定でサイドカー URL に `http://127.0.0.1:8410` を指定し、ペイン/タブで `http://127.0.0.1:8410/ui` を読み込んでください。

### 3. 使い方

1. 起動後、ブラウザまたは KASUGAI の WebView で `http://127.0.0.1:8410/ui` を開きます。
2. 「設定」タブで Box の `クライアントID`・`クライアントシークレット`・`Box API 検索件数`・`サイドカー待ち受けポート`・`MCP サーバーURL` 等を入力し「保存」を押してください。これらは OS の Keyring に保存されます。
3. 「ログイン」タブで Box OAuth 自動ログイン、またはデベロッパートークンでログインしてください。
4. 「Photo」タブで `対象フォルダURL` と `出力フォルダ名` を入力し、`実行` ボタンを押すと、指定フォルダ以下の画像ファイルを再帰的に取得し、EXIF 情報から位置情報・撮影日を抽出します。
5. 「Box API」タブではチャット形式で Box の簡易情報（ユーザー、フォルダ内容、検索など）を取得できます。
6. 処理が完了すると、CSV と GeoJSON が出力されます。既定では `ドキュメント/box_photo_geo_url/output/` に保存されます。

---

## 出力ファイル

- `box_photos.csv`  
  画像ファイル名、パス、緯度、経度、撮影日、Box 共有 URL を含む CSV
- `box_photos.geojson`  
  QGIS 等で利用できる GeoJSON ファイル（EPSG:4326、url 列付き）

---

## QGIS での利用例

- 「url」列を QGIS の「アクション」や「HTML ポップアップ」で利用し、写真を Web 表示できます。
  - 例: アクションに `[% "url" %]` を設定し Web ブラウザで画像を開く
  - 例: HTML ポップアップに `<img src="[% "url" %]" width="400">` など

---

## 開発者向け情報

### 簡易起動スクリプト

```sh
# 開発モード
cd C:\devin\kasugai_box
python run.py

# リリースビルド
python run.py -B

# リリース版を起動
python run.py --release
```

### リリースビルド

```sh
cd C:\devin\kasugai_box\server
cargo build --release
```

ビルドが完了すると、`server/target/release/kasugai_box.exe` が生成されます。

---

## 注意事項

- 画像の EXIF 情報は画像ファイル自体に埋め込まれているため、**画像をダウンロードせずに EXIF 情報を取得することはできません**。
- Box API は画像のメタデータとして EXIF 情報を直接返すエンドポイントを提供しません。
- EXIF の GPS 情報は WGS84（EPSG:4326）を前提としています。
- 取得した `access_token` の有効期限切れ時は、実行時に毎回新しいトークンを取得するため再入力は不要です。
- OAuth コールバックは `http://localhost:8000/callback` を使用します。起動時に一時的に `127.0.0.1:8000` をリッスンします。

---

## 参考

- [KASUGAI 外部連携方針](../kasugai/KASUGAI_INTEGRATION_POLICY.md)
- [Box API ドキュメント](https://developer.box.com/guides/authentication/oauth2/)
- [QGIS アクション/HTML ポップアップ](https://docs.qgis.org/ja/docs/user_manual/working_with_vector/actions.html)
