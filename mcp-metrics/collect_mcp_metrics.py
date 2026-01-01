#!/usr/bin/env -S uv run python
"""Collect raw MCP initialize/tools.list responses and save to mcp-metrics."""

from __future__ import annotations

import argparse
import json
import select
import subprocess
import time
from pathlib import Path


def send(pipe, obj):
    line = json.dumps(obj, separators=(",", ":"))
    pipe.write(line + "\n")
    pipe.flush()


def read_line(pipe, timeout: float) -> str:
    fd = pipe.fileno()
    ready, _, _ = select.select([fd], [], [], timeout)
    if not ready:
        raise TimeoutError("Timed out waiting for server response")
    return pipe.readline()


def read_response(pipe, target_id: int, timeout: float) -> tuple[str, dict]:
    start = time.time()
    while True:
        if time.time() - start > timeout:
            raise TimeoutError(f"Timed out waiting for id={target_id}")
        line = read_line(pipe, timeout=timeout)
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("id") == target_id:
            return line, msg


def resolve_binary(path: str | None) -> str:
    if path:
        return path
    default = Path("target/debug/lsp-mcp")
    if default.exists():
        return str(default)
    return "lsp-mcp"


def main() -> None:
    parser = argparse.ArgumentParser(description="Capture MCP raw responses.")
    parser.add_argument(
        "--workspace-root",
        default=str(Path.cwd()),
        help="Workspace root (default: current directory)",
    )
    parser.add_argument(
        "--bin",
        default=None,
        help="Path to lsp-mcp binary (default: target/debug/lsp-mcp or PATH)",
    )
    parser.add_argument(
        "--out-dir",
        default=str(Path("mcp-metrics")),
        help="Output directory (default: mcp-metrics)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=60.0,
        help="Timeout in seconds for each response (default: 60)",
    )
    args = parser.parse_args()

    binary = resolve_binary(args.bin)
    workspace = str(Path(args.workspace_root))
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    proc = subprocess.Popen(
        [binary, "--workspace-root", workspace],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )

    try:
        init_req = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "prompt-size-probe", "version": "0.1.0"},
            },
        }
        send(proc.stdin, init_req)
        init_line, _ = read_response(proc.stdout, 1, timeout=args.timeout)

        send(proc.stdin, {"jsonrpc": "2.0", "method": "notifications/initialized"})

        send(proc.stdin, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        tools_line, _ = read_response(proc.stdout, 2, timeout=args.timeout)

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.kill()

    init_path = out_dir / "initialize.json"
    tools_path = out_dir / "tools_list.json"
    init_path.write_text(init_line, encoding="utf-8")
    tools_path.write_text(tools_line, encoding="utf-8")

    print(
        json.dumps(
            {
                "initialize_path": str(init_path),
                "tools_list_path": str(tools_path),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
