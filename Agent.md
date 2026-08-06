# インストーラー・ショートカット作成仕様書

本ドキュメントは、Rust 製の API/サイドカー型アプリを Windows 向けに配布する際の、NSIS インストーラーと「コンソール非表示 + ブラウザ起動」ショートカットの設計・実装パターンをまとめたものです。

`kasugai_box` の実装をベースとしており、他のアプリへの流用を目的としています。

## 1. 前提・対象

- Windows 10/11 向け配布
- Rust（axum など）で実装された HTTP サービス
- 配布物は単一 EXE + NSIS インストーラー（`download/*.zip`、`download/*_setup.exe`）
- UI はブラウザ/WebView 上で表示される

## 2. アプリケーション側の設計

### 2.1 Windows サブシステム化

`server/src/main.rs` の先頭に以下を追加し、EXE 起動時にコンソール（DOS）ウィンドウを表示しないようにします。

```rust
#![windows_subsystem = "windows"]
```

注意：ログをコンソールに出力する場合は `console` サブシステムのままにするか、ファイル/トレースへ出力するようにします。

### 2.2 多重起動防止 + ブラウザ起動

以下の動作を `main()` 内に実装します。

- ポートが使用中なら `/health` へアクセスし、既存インスタンスがいるか判定
- 既存インスタンスがいる場合は、`/api/v1/server/stop` を呼び出して停止し、ポートが解放されるまで待ってから新規インスタンスを起動する
- 既存インスタンスがなくポートが使用できない場合はエラー終了
- `--open-browser` フラグが付いていれば、新規起動後に既定ブラウザで `http://127.0.0.1:{port}/ui` を開く

```rust
let open_browser = std::env::args().any(|a| a == "--open-browser");
let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
let listener = 'bind: loop {
    match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => break 'bind l,
        Err(e) => {
            let health_url = format!("http://127.0.0.1:{}/health", port);
            let existing = reqwest::get(&health_url)
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !existing {
                eprintln!("ポート {} で起動できません: {}", port, e);
                std::process::exit(1);
            }
            println!("既にポート {} で起動しています。古いインスタンスを停止します。", port);
            let stop_url = format!("http://127.0.0.1:{}/api/v1/server/stop", port);
            let _ = reqwest::Client::new().post(&stop_url).send().await;
            for _ in 1..=60 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if let Ok(l) = tokio::net::TcpListener::bind(addr).await {
                    println!("ポート {} を確保しました。新しいインスタンスを起動します。", port);
                    break 'bind l;
                }
            }
            eprintln!("ポート {} の解放を待ちましたが、起動できません: {}", port, e);
            std::process::exit(1);
        }
    }
};
```

新規起動時のブラウザ自動起動：

```rust
if open_browser {
    let open_url = format!("http://127.0.0.1:{}/ui", port);
    let health_url = format!("http://127.0.0.1:{}/health", port);
    tokio::spawn(async move {
        for _ in 0..60 {
            if let Ok(resp) = reqwest::get(&health_url).await {
                if resp.status().is_success() {
                    let _ = opener::open(&open_url);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });
}
```

なお、この多重起動防止はバージョンアップ時にも利用します。新しい EXE を同じ `C:\<vendor>\<app>` へ上書き配置して起動すれば、旧プロセスは自動的に停止するため、インストーラー側で強制終了する処理を別途用意する必要はありません。

### 2.3 EXE アイコンの埋め込み

1. `server/assets/icon.ico` にマルチサイズ ICO を配置（16, 32, 48, 64, 128, 256）
2. `server/Cargo.toml` に build-dependencies を追加

```toml
[build-dependencies]
winres = "0.1.12"
```

3. `server/build.rs` を新規作成

```rust
fn main() -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use winres::WindowsResource;
        WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()?;
    }
    Ok(())
}
```

## 3. アイコンの作成と配置

元画像（PNG など）から 16x16 〜 256x256 のマルチサイズ ICO を生成し、以下の場所に配置します。

- `web/favicon.ico` — Web UI 用ファビコン
- `installer/icon.ico` — NSIS インストーラー用アイコン
- `server/assets/icon.ico` — EXE リソース用アイコン

Web UI では `web/index.html` の `<head>` に以下を追加し、サーバー側で `/favicon.ico` を配信します。

```html
<link rel="icon" type="image/x-icon" href="/favicon.ico" />
```

サーバー側での配信例：

```rust
const FAVICON_ICO: &[u8] = include_bytes!("../../web/favicon.ico");

async fn serve_favicon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/x-icon")], FAVICON_ICO)
}
```

## 4. NSIS インストーラー仕様

### 4.1 基本構成

- MUI2 を使用したモダン UI
- Unicode 対応
- DPIAware
- 管理者権限を要求しないユーザー権限実行（`RequestExecutionLevel user`）
- 既定インストール先：`C:\<vendor>\<app>` （ユーザーが書き込み権限を持つディレクトリ。例：`C:\kasugai\kasugai_box`）

### 4.2 マルチサイズ ICO の明示

`installer/icon.ico` が 16x16 〜 256x256 のマルチサイズであることをコメントで明示します。

```nsis
; icon.ico は 16x16 から 256x256 までのマルチサイズ ICO を含む
!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
```

### 4.3 ショートカットのターゲット

スタートメニュー・デスクトップの両ショートカットは、EXE に `--open-browser` 引数を渡します。
アイコンは EXE 内の埋め込みアイコンを使用します。

```nsis
CreateShortcut "$SMPROGRAMS\<app>\<app>.lnk" \
    "$INSTDIR\<app>.exe" \
    "--open-browser" \
    "$INSTDIR\<app>.exe" 0 SW_SHOWNORMAL "" "" "$INSTDIR"
```

### 4.4 インストール完了ページ

完了時の「今すぐ実行」チェックボックスでも `--open-browser` を付けて起動します。

```nsis
!define MUI_FINISHPAGE_RUN "$INSTDIR\<app>.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--open-browser"
```

### 4.5 デスクトップショートカット

`MUI_PAGE_FINISH` の「README を表示」代替として、デスクトップショートカット作成機能を設ける場合：

```nsis
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Create desktop shortcut"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateDesktopShortcut
!insertmacro MUI_PAGE_FINISH
```

```nsis
Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\<app>.lnk" \
      "$INSTDIR\<app>.exe" \
      "--open-browser" \
      "$INSTDIR\<app>.exe" 0 SW_SHOWNORMAL "" "" "$INSTDIR"
FunctionEnd
```

## 5. ビルド・配布フロー

```powershell
cd <project_root>
python run.py -B
# または
python run.py b
# または
cd server
cargo build --release
cd ..
makensis installer\<app>.nsi
```

`run.py -B` または `python run.py b` であれば以下を一括実行するのが推奨されます。

1. `cargo build --release`
2. `download/<app>.zip` へ EXE を圧縮
3. NSIS インストーラーが利用可能なら `download/<app>_setup.exe` を生成

## 6. 検証観点

- ショートカットをダブルクリックするとブラウザが開く
- 2 回目以降のダブルクリックで新しいプロセスが増えない
- インストーラー自体のアイコンが正しく表示される
- タスクバー/スタートメニューのアイコンが正しく表示される
- ファビコンがブラウザタブに表示される
- アンインストールでスタートメニュー・デスクトップのショートカットが削除される

## 7. 参考ファイル（kasugai_box の実装例）

- `installer/kasugai_box.nsi`
- `server/src/main.rs`
- `server/build.rs`
- `server/Cargo.toml`
- `web/index.html`
- `run.py`

## 8. run.py 仕様書

`run.py` はプロジェクトルートから実行するラッパースクリプトです。

| 引数 | 動作 |
|---|---|
| （引数なし） | `cargo run` で開発モードを起動 |
| `-b`, `-B`, `--build` | リリースビルド、ZIP 作成、可能であればインストーラー作成 |
| `b` または `B`（位置引数） | `--build` と同じ |
| `--installer` | リリースビルド後に NSIS インストーラーを作成 |
| `--release` | `target/release/<app>.exe` を起動 |

`python run.py -B` は以下を一括実行します。

1. `cargo build --release`
2. `download/<app>.zip` へ EXE を圧縮
3. `makensis.exe` が利用可能なら `download/<app>_setup.exe` を生成

`b/B` を位置引数に使えるため、`python run.py b` でも同じビルド・配布が実行されます。

## 9. ダウンロード警告対策（インストーラー ZIP 化）

### 9.1 背景

ブラウザ（Chrome / Edge など）は、`.exe` ファイルを「よくダウンロードされていない」「危険な可能性がある」として警告し、ダウンロードをブロックすることがあります。これを回避するため、インストーラー EXE を ZIP 化して配布します。

### 9.2 配布物の例

- `download/<app>.zip` — アプリ本体の配布用 ZIP。自動更新（updater）用 `latest.json` はこちらを指します。
- `download/<app>_setup.exe` — NSIS インストーラー本体。
- `download/<app>_setup.zip` — 上記インストーラーを ZIP 化したもの。ユーザー向け手動ダウンロード用。
- `download/latest.json` — updater 用。`windows-x86_64` の `url` は `main` ブランチの `download/<app>.zip` を指す `https://raw.githubusercontent.com/<owner>/<repo>/main/download/<app>.zip` とします。

### 9.3 実装メモ

Tauri の bundle `targets` には `zip` が存在しないため、`tauri.conf.json` で `targets: ["nsis"]`（または `["msi"]`）としておき、ビルド後に `run.py` などでインストーラー EXE を ZIP 化します。

```python
import zipfile

# インストーラーは download/<app>_setup.exe として生成されている想定
dest_zip = os.path.join(download_dir, '<app>_setup.zip')
with zipfile.ZipFile(dest_zip, 'w', zipfile.ZIP_DEFLATED) as zf:
    zf.write(dest_installer, os.path.basename(dest_installer))
print(f"[Kasugai] インストーラー ZIP を生成しました: {dest_zip}")
```

### 9.4 注意点

- ZIP 内にはインストーラー EXE 1 つを入れます。
- 自動更新が `<app>.zip` を直接ダウンロード・差し替える場合、署名はその ZIP に対する `.sig` ファイルを使用します。NSIS インストーラー EXE 自体の改竄防止にも個別の `.sig` を付与できます。
- 完全に SmartScreen 警告を消すには EV コードサイニング証明書が必要です。

## 10. バージョンアップ仕様

### 10.1 正本

バージョン番号の正本は `server/Cargo.toml` の `package.version` とします。他の設定ファイルはこの値を基準に更新・検証し、バージョン管理は `main` ブランチで完結させます。

### 10.2 更新対象ファイル

バージョンアップ時に `server/Cargo.toml` と同じバージョンに合わせるファイルは以下の通りです。

| ファイル | 更新箇所 | 例 |
|---|---|---|
| `server/Cargo.toml` | `package.version` | `version = "0.5.3"` |
| `installer/<app>.nsi` | `VIProductVersion` / `FileVersion` | `VIProductVersion "0.5.3.0"`、`VIAddVersionKey "FileVersion" "0.5.3"` |
| `download/latest.json` | `version` / `notes` | `"version": "0.5.3"`、`"notes": "KASUGAI <app> 0.5.3"` |
| `README.md` | 現在バージョン記述 | `現在のバージョンは **0.5.3** です。` |
| `CHANGELOG.md` | 最新のバージョン見出しと比較リンク | `## [0.5.3] - 2026-08-05`、`[0.5.3]: ...compare/v0.5.2...v0.5.3` |

`server/Cargo.lock` は `cargo build` 時に自動的に更新されます。

### 10.3 リリース手順

1. `server/Cargo.toml` の `package.version` を更新する
2. 上記の各ファイルを同じバージョンに合わせて更新する
3. `python run.py -b` でリリースビルド、ZIP、NSIS インストーラーを作成する
4. ソース・ビルド成果物を GitHub の `main` ブランチへ commit/push する
5. 必要に応じて `vX.Y.Z` タグを作成・push する

### 10.4 自動整合性チェック

複数ファイルのバージョンズレを防ぐため、`run.py` のリリースビルド前に `check_versions()` を実行することを推奨します。`check_versions()` の実装例は `kasugai_canvas/run.py` を参照してください。

`check_versions()` は `server/Cargo.toml` のバージョンを読み取り、`installer/<app>.nsi`、`download/latest.json`、`README.md`、`CHANGELOG.md` のバージョンが一致しているかを検証します。一致しない場合はどのファイルがずれているかを出力してビルドを中断します。

