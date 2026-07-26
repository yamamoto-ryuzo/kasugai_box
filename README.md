# kasugai_box

KASUGAI 用 API サイドカー。Box フォルダ内画像の EXIF 抽出（CSV/GeoJSON 出力）、Box API チャット、Box OAuth ログインを HTTP/REST + MCP で提供します。

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
