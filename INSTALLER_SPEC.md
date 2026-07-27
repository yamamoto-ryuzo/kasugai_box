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
- 管理者権限で実行（`RequestExecutionLevel admin`）
- 既定インストール先：`C:\<vendor>\<app>`

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
cd server
cargo build --release
cd ..
makensis installer\<app>.nsi
```

`run.py -B` であれば以下を一括実行するのが推奨されます。

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
