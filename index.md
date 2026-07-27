# kasugai_box

**Rust × axum** で実装された **KASUGAI 用 API サイドカー** です。複数の Box フォルダ内の画像を一覧し、Box サーバー側の埋め込みメタデータから緯度経度・撮影日を取得して、共有リンク付きで一覧化・GeoJSON/CSV 出力を行います。

> 写真の位置情報は Box サーバー側の `embedded_metadata` 表現から取得します。位置情報が含まれない画像は空欄になります。

---

## タブ別の使い方

UI は以下のタブに分かれています。

### Photo

Box フォルダ内の画像を再帰的に走査し、緯度・経度・撮影日を CSV・GeoJSON に出力します。

- **フォルダURL**: 対象フォルダの URL または ID。改行またはカンマ区切りで複数指定できます。
- **出力フォルダ名**: CSV・GeoJSON の出力先。未指定時は `c:/kasugai/box/photo`。
- **実行**: 処理を開始します。長時間の場合があります。
- **停止**: 実行中の処理を中断できます。
- **出力フォルダを開く**: 出力先を OS のファイルマネージャーで開きます。
- 処理中は「何をしているか」「何件目か」が進捗表示されます。

### Box API

チャット形式で Box API を操作できます。利用可能なコマンド例:

- `help` / `me`
- `folder <id>` — フォルダ内容の一覧
- `search <keyword>` — ファイル名・メタデータ・文書内テキストを検索

### MCP

外部 MCP サーバーに接続し、ツール一覧の取得・実行ができます。設定は「管理」タブの「設定」で行います。

### APIヘルプ

REST API / MCP の仕様と例を確認できます。完全な仕様は [openapi.yaml](./openapi.yaml) を参照してください。

### 管理

- **ログイン**: Box OAuth 自動ログイン、またはデベロッパートークンでログインします。
- **設定**: Box クライアントID/シークレット、デベロッパートークン、検索件数、MCP サーバーURL 等を Keyring へ保存します。
- **サイドカー**: 現在の待ち受けポートの確認、サイドカーの停止・再起動ができます。
- **バージョン**: 現在バージョンと最新バージョンの確認、アップデートができます。

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
| ジョブ停止 | `POST /api/v1/jobs/{id}/cancel` |
| 出力フォルダを開く | `POST /api/v1/open-folder` |
| Box API チャット | `POST /api/v1/box/chat` |
| 外部 MCP ツール操作 | `GET /api/v1/mcp-client/tools`, `POST /api/v1/mcp-client/call` |
| サイドカー起動 | `POST /api/v1/server/restart` |
| AI エージェント用 MCP | `POST /mcp` |

---

## 技術スタックと役割

| システム | 技術スタック | 主な役割 |
| :--- | :--- | :--- |
| **kasugai_box**（本サイドカー） | Rust × axum | Box 連携、写真メタデータ取得・CSV/GeoJSON 出力、REST API / MCP 提供 |
| **KASUGAI 本体** | Tauri v2 × Rust | ウィンドウ/WebView 制御、サイドカーの起動管理、複数サイトの統合 UI |

KASUGAI 本体は **ブラウザ/タブを操作するため** の Tauri アプリです。`kasugai_box` はその横で動く独立した HTTP サービスで、KASUGAI の WebView から `http://127.0.0.1:8410/ui` を読み込み、または AI エージェントが `POST /mcp` で利用します。

### axum とは

[axum](https://github.com/tokio-rs/axum) は **Rust の非同期 HTTP フレームワーク** です。`tokio` 上で動作し、軽量・高機能な Web サーバーを単一バイナリにまとめやすいのが特徴です。

`kasugai_box` で axum を選んだ理由は次の通りです：

| 理由 | 説明 |
| :--- | :--- |
| **非同期処理** | `tokio` と連携し、Box API 通信や長時間の写真処理を効率よく扱える |
| **ルーティングが簡潔** | `Router::new().route("/health", get(health))` のように直感的に書ける |
| **ミドルウェア** | CORS 対応や認証レイヤーを簡単に追加できる |
| **単一バイナリ化** | 依存ライブラリごと `kasugai_box.exe` 1 つにまとめられる |
| **MCP 対応** | JSON-RPC 2.0 / Streamable HTTP エンドポイントを普通のルートとして実装できる |

---

## ダウンロード

| 種別 | 入手先 |
| :--- | :--- |
| リリースビルド済みバイナリ | [download/kasugai_box.zip](./download/kasugai_box.zip) |
| NSIS インストーラー | [download/kasugai_box_setup.exe](./download/kasugai_box_setup.exe) |
| ソースコード | [GitHub リポジトリ](https://github.com/yamamoto-ryuzo/kasugai_box) |

インストーラー・ショートカットの仕様は [INSTALLER_SPEC.md](./INSTALLER_SPEC.md) を参照してください。

---

## ドキュメント索引

| ドキュメント | 内容 |
| :--- | :--- |
| [OpenAPI 仕様](./openapi.yaml) | REST API / MCP エンドポイントの完全な仕様 |
| [KASUGAI 外部連携方針](../kasugai/KASUGAI_INTEGRATION_POLICY.md) | KASUGAI サイドカー全体の設計方針 |
| [インストーラー・ショートカット仕様](./INSTALLER_SPEC.md) | Windows 配布用の標準パターン |
| [Changelog](./CHANGELOG.md) | リリース履歴と変更点 |
| [GitHub リポジトリ](https://github.com/yamamoto-ryuzo/kasugai_box) | ソースコード・Issue |

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

推奨：`python run.py -B` で `download/kasugai_box.zip` を一括生成・配置します。

```sh
cd C:\devin\kasugai_box
python run.py -B
```

または、直接 cargo を使う場合：

```sh
cd C:\devin\kasugai_box\server
cargo build --release
```

ビルドが完了すると、`server/target/release/kasugai_box.exe` が生成されます。`python run.py -B` 実行時は同時に `download/kasugai_box.zip` へ圧縮・配置されます。

---

## 注意事項

- 写真の位置情報は Box サーバー側の `embedded_metadata` 表現から取得します。取得できない画像は緯度・経度を空欄として出力します。
- 同じポートでは複数の `kasugai_box` を起動できません。新しいプロセスを起動すると、既存インスタンスを自動的に停止してポートを確保します。
- 測地座標系は WGS84（EPSG:4326）を前提としています。
- 取得した `access_token` の有効期限切れ時は、実行時に毎回新しいトークンを取得するため再入力は不要です。
- OAuth コールバックは `http://localhost:8000/callback` を使用します。起動時に一時的に `127.0.0.1:8000` をリッスンします。

---

## ライセンス

- `kasugai_box` 本体: [MIT License](./LICENSE)
- サードパーティーライブラリのライセンス: [THIRD-PARTY-LICENSES.md](./THIRD-PARTY-LICENSES.md)

依存ライブラリを `cargo-license` で確認した結果、`GPL` / `LGPL` 系の強いコピーレフトは含まれておらず、MIT ライセンスで公開できます。

---

## 参考

- [KASUGAI 外部連携方針](../kasugai/KASUGAI_INTEGRATION_POLICY.md)
- [Box API ドキュメント](https://developer.box.com/guides/authentication/oauth2/)
- [QGIS アクション/HTML ポップアップ](https://docs.qgis.org/ja/docs/user_manual/working_with_vector/actions.html)


---

**免責事項**: 本システムは個人のPCで作成・テストされたものです。ご利用によるいかなる損害も責任を負いません。
<p align="center">
  <a href="https://giphy.com/explore/free-gif" target="_blank">
    <img src="https://github.com/yamamoto-ryuzo/QGIS_portable_3x/raw/master/imgs/giphy.gif" width="500" title="avvio QGIS">
  </a>
</p>