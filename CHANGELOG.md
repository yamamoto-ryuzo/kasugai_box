# Changelog

## [Unreleased]

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

### Security
- クライアントID・シークレット・Subject ID などの認証情報を OS キーリングに保存し、平文の `config.json` 保存を廃止
