# kasugai_box

**Rust × axum** で実装された **KASUGAI 用 API サイドカー** です。Box フォルダ内画像の EXIF 抽出（CSV/GeoJSON 出力）、Box API チャット、Box OAuth ログインを HTTP/REST + MCP で提供します。

## KASUGAI との違い

| システム | 技術スタック | 主な役割 |
| :--- | :--- | :--- |
| **kasugai_box** | Rust × axum | Box 連携、EXIF 処理、REST API / MCP サーバー |
| **KASUGAI 本体** | Tauri v2 × Rust | ウィンドウ/WebView 制御、サイドカー起動、ブラウザ統合 UI |

KASUGAI 本体は **ブラウザ/タブをいろいろ操作するため** の Tauri アプリです。`kasugai_box` はその横で動く独立した HTTP サービスで、`127.0.0.1:8410` で待ち受けます。

## ダウンロード

- **[download/kasugai_box.exe](./download/kasugai_box.exe)** — `python run.py -B` でビルド・配置されます

## ドキュメント（GitHub Pages）

詳細なドキュメントは **GitHub Pages** で確認してください。

**[→ kasugai_box ドキュメントを開く](https://yamamoto-ryuzo.github.io/kasugai_box/)**

（`index.md` を GitHub Pages のトップページとして利用しています）

## 主なリンク

| リンク | 内容 |
| :--- | :--- |
| [OpenAPI 仕様](./openapi.yaml) | REST API / MCP エンドポイントの完全な仕様 |
| [Changelog](./CHANGELOG.md) | リリース履歴と変更点 |
| [KASUGAI 外部連携方針](../kasugai/KASUGAI_INTEGRATION_POLICY.md) | KASUGAI サイドカー全体の設計方針 |
| [GitHub リポジトリ](https://github.com/yamamoto-ryuzo/kasugai_box) | ソースコード・Issue |

## クイックスタート

```sh
cd C:\devin\kasugai_box
python run.py
```

既定ポート `8410` で `http://127.0.0.1:8410/ui` が開きます。詳細は上記 GitHub Pages を参照してください。
