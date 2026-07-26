# Changelog

## [Unreleased]

### Added
- MCP タブ: 設定タブの Box 認証情報を使ったチャット形式の Box 情報取得機能
- OS 資格情報保護（Keyring）による設定保存

### Changed
- アプリタイトルを `kasugai_box` に統一
- タブ構成を Photo / MCP / 設定 に整理
- Photo タブの説明文を Photo タブ内に移動
- 設定は Photo・MCP 両方で共有

### Security
- クライアントID・シークレット・Subject ID などの認証情報を OS キーリングに保存し、平文の `config.json` 保存を廃止
