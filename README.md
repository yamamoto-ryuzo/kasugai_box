# kasugai_box

**Rust × axum** で実装された **KASUGAI 用 API サイドカー** です。Box フォルダ内画像の EXIF 抽出（CSV/GeoJSON 出力）、Box API チャット、Box OAuth ログインを HTTP/REST + MCP で提供します。

## KASUGAI との違い

| システム | 技術スタック | 主な役割 |
| :--- | :--- | :--- |
| **kasugai_box** | Rust × axum | Box 連携、EXIF 処理、REST API / MCP サーバー |
| **KASUGAI 本体** | Tauri v2 × Rust | ウィンドウ/WebView 制御、サイドカー起動、ブラウザ統合 UI |

KASUGAI 本体は **ブラウザ/タブをいろいろ操作するため** の Tauri アプリです。`kasugai_box` はその横で動く独立した HTTP サービスで、Rust 非同期フレームワーク **axum** を使い `127.0.0.1:8410` で待ち受けます。詳細な axum の概要は GitHub Pages を参照してください。

## ダウンロード

- **[download/kasugai_box.zip](./download/kasugai_box.zip)** — `python run.py b` でビルド・配置されます（ZIP 内の `kasugai_box.exe` を展開して利用してください）
- **[download/kasugai_box_setup.exe](./download/kasugai_box_setup.exe)** — `python run.py b` でビルドされる NSIS インストーラーです。起動してウィザードに従うと Windows のスタートメニューに登録されます

## インストール

### 前提条件

- Windows（ビルド済みバイナリ・NSIS インストーラーは Windows 向け）
- [Rust](https://www.rust-lang.org/tools/install)（ソースからビルドする場合）
- [Python 3](https://www.python.org/downloads/)（`run.py` ランチャーを使う場合）

### ビルド済みバイナリで利用する場合

1. [download/kasugai_box.zip](./download/kasugai_box.zip) をダウンロードします。
2. 任意のフォルダに展開します。
3. `kasugai_box.exe` をダブルクリック、またはコマンドラインから実行します。

### ソースからビルドする場合

```sh
cd C:\devin\kasugai_box
python run.py -B
```

または、直接 `cargo` を使う場合：

```sh
cd C:\devin\kasugai_box\server
cargo build --release
```

ビルド後、`server/target/release/kasugai_box.exe` が生成されます。

### NSIS インストーラーを作成する場合

NSIS をインストール済みの環境で、以下を実行します。

```sh
cd C:\devin\kasugai_box
python run.py --installer
```

または、直接 `makensis` を使う場合：

```sh
cd C:\devin\kasugai_box
makensis installer\kasugai_box.nsi
```

`download/kasugai_box_setup.exe` が生成されます。これを実行すると Windows のスタートメニューに登録されてインストールされます。

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

## ライセンス

- `kasugai_box` 本体: [MIT License](./LICENSE)
- サードパーティーライブラリのライセンス: [THIRD-PARTY-LICENSES.md](./THIRD-PARTY-LICENSES.md)

依存ライブラリを確認した結果、`GPL` / `LGPL` 系の強いコピーレフトは含まれておらず、MIT ライセンスで公開できます。
