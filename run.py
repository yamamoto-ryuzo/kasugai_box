#!/usr/bin/env python3
"""kasugai_box API sidecar ランチャー"""
import argparse
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SERVER_DIR = ROOT / "server"
DOWNLOAD_DIR = ROOT / "download"
TARGET_EXE = SERVER_DIR / "target" / "release" / "kasugai_box.exe"
DOWNLOAD_ZIP = DOWNLOAD_DIR / "kasugai_box.zip"


def _zip_to_download():
    """リリースビルド後、ダウンロード用に zip を download/ に作成する"""
    DOWNLOAD_DIR.mkdir(parents=True, exist_ok=True)
    if not TARGET_EXE.exists():
        print(f"ビルド済み実行ファイルが見つかりません: {TARGET_EXE}", file=sys.stderr)
        return
    with zipfile.ZipFile(DOWNLOAD_ZIP, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.write(TARGET_EXE, arcname=TARGET_EXE.name)
    print(f"ZIP を作成しました: {DOWNLOAD_ZIP}")


def run_dev():
    """開発モードで起動 (cargo run)"""
    subprocess.run(["cargo", "run"], cwd=SERVER_DIR)


def build_release():
    """リリースビルド (cargo build --release)"""
    subprocess.run(["cargo", "build", "--release"], cwd=SERVER_DIR)
    _zip_to_download()


def _find_makensis():
    """makensis.exe のパスを探す"""
    exe = shutil.which("makensis") or shutil.which("makensis.exe")
    if exe:
        return Path(exe)
    for candidate in [
        Path(r"C:\Program Files\NSIS\makensis.exe"),
        Path(r"C:\Program Files (x86)\NSIS\makensis.exe"),
    ]:
        if candidate.exists():
            return candidate
    return None


def build_installer():
    """NSIS インストーラーを作成する"""
    makensis = _find_makensis()
    if makensis is None:
        print("makensis.exe が見つかりません。NSIS をインストールして PATH を通してください。", file=sys.stderr)
        print("https://nsis.sourceforge.io/Download", file=sys.stderr)
        sys.exit(1)
    nsi = ROOT / "installer" / "kasugai_box.nsi"
    if not nsi.exists():
        print(f"インストーラースクリプトが見つかりません: {nsi}", file=sys.stderr)
        sys.exit(1)
    subprocess.run([str(makensis), str(nsi)], cwd=ROOT)
    print(f"インストーラーを作成しました: {DOWNLOAD_DIR / 'kasugai_box_setup.exe'}")


def run_release():
    """リリースビルド済み実行ファイルを起動"""
    exe = TARGET_EXE
    if not exe.exists():
        print(f"リリース実行ファイルが見つかりません: {exe}", file=sys.stderr)
        print("先に 'python run.py -B' または 'cargo build --release' を実行してください。", file=sys.stderr)
        sys.exit(1)
    subprocess.run([str(exe)], cwd=ROOT)


def main():
    parser = argparse.ArgumentParser(description="kasugai_box API sidecar ランチャー")
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "-b",
        "-B",
        "--build",
        action="store_true",
        help="リリースビルドを実行 (cargo build --release) し download/ に ZIP を作成",
    )
    group.add_argument(
        "--release",
        action="store_true",
        help="リリースビルド済みの kasugai_box.exe を起動（未指定時は cargo run）",
    )
    parser.add_argument(
        "--installer",
        action="store_true",
        help="リリースビルド後に NSIS インストーラー (download/kasugai_box_setup.exe) を作成",
    )
    args = parser.parse_args()

    if args.installer:
        build_release()
        build_installer()
    elif args.build:
        build_release()
    elif args.release:
        run_release()
    else:
        run_dev()


if __name__ == "__main__":
    main()
