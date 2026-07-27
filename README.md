# kasugai_box

**Rust × axum** で実装された **KASUGAI 用 API サイドカー** です。  
Box フォルダ内の画像を一覧し、Box サーバー側の埋め込みメタデータから緯度・経度・撮影日を取得して、CSV・GeoJSON 出力を行います。

## ドキュメント

詳細なドキュメント、セットアップ手順、API 仕様は **GitHub Pages** を参照してください。

**[→ kasugai_box ドキュメントを開く](https://yamamoto-ryuzo.github.io/kasugai_box/)**

- [index.md](./index.md) — GitHub Pages のトップページ（詳細ドキュメント）
- [INSTALLER_SPEC.md](./INSTALLER_SPEC.md) — Windows インストーラー・ショートカット仕様
- [CHANGELOG.md](./CHANGELOG.md) — リリース履歴
- [openapi.yaml](./openapi.yaml) — REST API / MCP 仕様

## ライセンス

- `kasugai_box` 本体: [MIT License](./LICENSE)
- サードパーティーライブラリのライセンス: [THIRD-PARTY-LICENSES.md](./THIRD-PARTY-LICENSES.md)
