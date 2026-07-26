#!/usr/bin/env python3
"""kasugai_box API sidecar ランチャー"""
import argparse
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
    args = parser.parse_args()

    if args.build:
        build_release()
    elif args.release:
        run_release()
    else:
        run_dev()


if __name__ == "__main__":
    main()
