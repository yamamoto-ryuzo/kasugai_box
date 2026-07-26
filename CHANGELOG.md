# Changelog

## [Unreleased]

## [3.0.0] - 2026-07-26

メジャーアップデート。Tauri v2 デスクトップアプリから **API サイドカー（HTTP/REST/MCP サーバー）** へ再構成しました。`kasugai_box` は `127.0.0.1:8410` で待ち受ける独立したサイドカーとなり、KASUGAI 本体は WebView または MCP クライアントとして利用します。

### Added
- API サイドカー化：axum 製 HTTP サーバーを実装（`server/`）
- `GET /health` で名前・バージョンを返すヘルスチェック
- `GET /ui` / `/main.js` / `/styles.css` で UI を同一オリジン配信
- `GET /openapi.yaml` で OpenAPI 仕様を提供
- `GET/POST /api/v1/config` で設定取得・更新（UI には平文を渡さない）
- `POST /api/v1/photos/process` で長時間処理を `202 Accepted` + ジョブ ID で非同期実行
- `GET /api/v1/jobs/{id}` でジョブ進捗・結果を取得
- `POST /api/v1/auth/box/login` でシステムブラウザ + `localhost:8000/callback` OAuth ログイン
- `POST /api/v1/auth/box/developer-token` でデベロッパートークン認証
- `POST /mcp` で自前の MCP サーバー（Streamable HTTP / JSON-RPC 2.0）を提供
- MCP ツール：`box_whoami`, `box_search`, `box_list_folder`, `box_create_shared_link`, `photos_process`, `job_status`
- `KASUGAI_BOX_PORT` 環境変数でポートを変更可能（既定 `8410`）
- 同一オリジン UI 用 fetch API への移行（`window.__TAURI__.core.invoke` を廃止）
- `GET/POST /api/v1/server/status|stop` で現在ポート表示と graceful shutdown 停止ボタンを UI に追加
- サイドカー待ち受けポートを keyring 保存の `config.port` に追加（UI 設定タブで変更可。環境変数 `KASUGAI_BOX_PORT` で上書き可）

### Changed
- Tauri v2 デスクトップアプリ構成を廃止し、独立した API サイドカーとして再構成
- 実行ファイル名を `kasugai_box.exe` に統一（`server/target/release/`）
- 画像処理ジョブを非同期化し UI 経由でリアルタイム進捗を表示
- `run.py` を `cargo tauri` から `cargo` ベースの起動・ビルドに変更

### Security
- API サイドカーは `127.0.0.1` のみでバインドし、`0.0.0.0` バインドを禁止
- API キー・アクセストークンはサイドカー内 keyring で管理し、UI には平文を渡さない
- UI には認証情報の保存有無フラグのみ返し、実値は保持しない

## [0.1.0] - 2026-07-26

### Added
- Box API タブ: 設定タブの Box 認証情報を使ったチャット形式の Box 情報取得機能
- MCP タブ: 設定した MCP サーバーに対して `tools/list` / `tools/call` を実行する機能
- OS 資格情報保護（Keyring）による設定保存
- Box API 検索結果に HTML リンク形式でルートからのパスとリンクを表示
- Box API `link <file|folder> <id>` コマンドでファイル・フォルダの共有リンクを作成
- `open_url` コマンドで OS デフォルトブラウザを開く
- チャットメッセージの内容をクリップボードにコピーする機能を追加

### Changed
- アプリタイトルを `kasugai_box` に統一
- タブ構成を Photo / Box API / MCP / 設定 に整理
- Photo タブの説明文を Photo タブ内に移動
- 設定は Photo・Box API・MCP 両方で共有
- タブパネルとチャットエリアをウィンドウサイズに応じて可変に調整
- コンテナをウィンドウ幅いっぱいに広がるように変更
- Box API 関連の内部変数名・コマンド名を `mcp_*` から `box_api_*` / `box_*` に整理
- チャットバブルのアシスタントメッセージを `innerHTML` に対応
- Box API 検索件数を 1〜200 件に拡大
- Box API 検索のページングを `limit` の倍数に修正
- Box API 検索件数の変更を即座に反映

### Security
- クライアントID・シークレット・Subject ID などの認証情報を OS キーリングに保存し、平文の `config.json` 保存を廃止
