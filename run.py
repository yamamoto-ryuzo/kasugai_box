#!/usr/bin/env python3
"""Box Photo Geo URL (Tauri) ランチャー"""
import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent

def run_dev():
    """開発モードで起動 (cargo tauri dev)"""
    subprocess.run(["cargo", "tauri", "dev"], cwd=ROOT)

def build_release():
    """リリースビルド (cargo tauri build)"""
    subprocess.run(["cargo", "tauri", "build"], cwd=ROOT)

def run_release():
    """リリースビルドの実行ファイルを起動"""
    exe = (
        ROOT
        / "src-tauri"
        / "target"
        / "release"
        / "box_photo_geo_url_rs.exe"
    )
    if not exe.exists():
        print(f"リリース実行ファイルが見つかりません: {exe}", file=sys.stderr)
        print("先に 'python run.py -B' または 'cargo tauri build' を実行してください。", file=sys.stderr)
        sys.exit(1)
    subprocess.run([str(exe)], cwd=ROOT)

def main():
    parser = argparse.ArgumentParser(description="Box Photo Geo URL ランチャー")
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "-b",
        "-B",
        "--build",
        action="store_true",
        help="リリースビルドを実行 (cargo tauri build)",
    )
    group.add_argument(
        "--release",
        action="store_true",
        help="リリース実行ファイルを起動（未指定時は cargo tauri dev）",
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
