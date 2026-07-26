#!/usr/bin/env python3
"""kasugai_box API sidecar ランチャー"""
import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SERVER_DIR = ROOT / "server"


def run_dev():
    """開発モードで起動 (cargo run)"""
    subprocess.run(["cargo", "run"], cwd=SERVER_DIR)


def build_release():
    """リリースビルド (cargo build --release)"""
    subprocess.run(["cargo", "build", "--release"], cwd=SERVER_DIR)


def run_release():
    """リリースビルドの実行ファイルを起動"""
    exe = SERVER_DIR / "target" / "release" / "kasugai_box.exe"
    if not exe.exists():
        print(f"リリース実行ファイルが見つかりません: {exe}", file=sys.stderr)
        print("先に 'python run.py -B' または 'cargo build --release' を実行してください。", file=sys.stderr)
        sys.exit(1)
    subprocess.run([str(exe)], cwd=SERVER_DIR)


def main():
    parser = argparse.ArgumentParser(description="kasugai_box API sidecar ランチャー")
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "-b",
        "-B",
        "--build",
        action="store_true",
        help="リリースビルドを実行 (cargo build --release)",
    )
    group.add_argument(
        "--release",
        action="store_true",
        help="リリース実行ファイルを起動（未指定時は cargo run）",
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
