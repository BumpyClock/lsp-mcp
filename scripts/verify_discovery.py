#!/usr/bin/env python3
"""Exercise discovery through the built MCP binary, using an isolated fake LSP.

Usage: python scripts/verify_discovery.py /path/to/lsp-mcp[.exe]
No installed language servers or network needed. On Windows, account-level
.lsp-mcp config must be absent because the OS home-directory API ignores HOME.
"""

import json
import os
import queue
import shlex
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path


def lsp_send(message):
    data = json.dumps(message).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
    sys.stdout.buffer.flush()


def fake_server():
    root = Path.cwd()
    with (root / ".fake-starts").open("a") as log:
        log.write("start\n")
    source = sys.stdin.buffer
    while True:
        headers = {}
        while True:
            line = source.readline()
            if not line:
                return
            if not line.strip():
                break
            key, value = line.decode().split(":", 1)
            headers[key.lower()] = value.strip()
        message = json.loads(source.read(int(headers["content-length"])))
        if "id" not in message:
            continue
        method = message.get("method")
        reply = {"jsonrpc": "2.0", "id": message["id"]}
        if method == "initialize":
            if (root / ".fake-fail").exists():
                reply["error"] = {
                    "code": -32603,
                    "message": "fixture initialization refused",
                }
            else:
                # Hold first contact long enough to observe initializing and concurrent requests.
                time.sleep(0.2)
                reply["result"] = {
                    "capabilities": {
                        "hoverProvider": True,
                        "workspaceSymbolProvider": True,
                    }
                }
        elif method == "textDocument/hover":
            if (root / ".fake-exit").exists():
                return
            if (root / ".fake-progress").exists():
                lsp_send(
                    {
                        "jsonrpc": "2.0",
                        "method": "$/progress",
                        "params": {
                            "token": "fixture",
                            "value": {"kind": "begin", "title": "fixture indexing"},
                        },
                    }
                )
                (root / ".fake-hover-started").write_text("started")
                time.sleep(2)
                lsp_send(
                    {
                        "jsonrpc": "2.0",
                        "method": "$/progress",
                        "params": {"token": "fixture", "value": {"kind": "end"}},
                    }
                )
            reply["result"] = {"contents": "discovery-probe-hover"}
        elif method == "workspace/symbol":
            reply["result"] = [
                {
                    "name": "discovery_probe",
                    "kind": 12,
                    "location": {
                        "uri": (root / "main.rs").as_uri(),
                        "range": {
                            "start": {"line": 0, "character": 3},
                            "end": {"line": 0, "character": 18},
                        },
                    },
                }
            ]
        elif method == "textDocument/definition":
            reply["result"] = []
        else:
            reply["result"] = None
        lsp_send(reply)


class Client:
    def __init__(self, binary, root, home, path=None):
        env = os.environ.copy()
        env.update(HOME=str(home), USERPROFILE=str(home))
        if path is not None:
            env["PATH"] = str(path)
        self.stderr = (root / ".mcp-stderr").open("w+")
        self.process = subprocess.Popen(
            [str(binary), "--workspace-root", str(root)],
            cwd=root,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr,
            text=True,
            encoding="utf-8",
        )
        assert self.process.stdin is not None and self.process.stdout is not None
        self.stdin = self.process.stdin
        self.stdout = self.process.stdout
        self.messages = queue.Queue()
        self.pending = {}
        self.sequence = 0
        threading.Thread(target=self.read_output, daemon=True).start()
        self.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "discovery-probe", "version": "1"},
            },
        )
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def read_output(self):
        for line in self.stdout:
            try:
                self.messages.put(json.loads(line))
            except ValueError:
                self.messages.put({"invalid_stdout": line})
        self.messages.put({"eof": True})

    def send(self, message):
        self.stdin.write(json.dumps(message) + "\n")
        self.stdin.flush()

    def begin(self, method, params):
        self.sequence += 1
        self.send(
            {"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params}
        )
        return self.sequence

    def finish(self, ident):
        deadline = time.monotonic() + 45
        while ident not in self.pending:
            item = self.messages.get(timeout=max(0.01, deadline - time.monotonic()))
            if "eof" in item or "invalid_stdout" in item:
                self.stderr.flush()
                raise AssertionError(
                    f"MCP transport failed: {item}; stderr={Path(self.stderr.name).read_text()}"
                )
            if "id" in item:
                self.pending[item["id"]] = item
        result = self.pending.pop(ident)
        assert "error" not in result, result
        return result["result"]

    def request(self, method, params):
        return self.finish(self.begin(method, params))

    def tool(self, name, arguments=None):
        return self.request("tools/call", {"name": name, "arguments": arguments or {}})

    def text(self, result):
        return "\n".join(item.get("text", "") for item in result.get("content", []))

    def health(self):
        return self.text(self.tool("health"))

    def wait_health(self, expected):
        deadline = time.monotonic() + 15
        while True:
            health = self.health()
            if expected in health:
                return health
            assert time.monotonic() < deadline, (expected, health)
            time.sleep(0.05)

    def close(self):
        self.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.stdout.close()
        self.stderr.close()


def verify(binary):
    if os.name == "nt":
        assert not (Path.home() / ".lsp-mcp" / ".lsp-mcp.json").exists(), (
            "Windows probe requires no account-level global config"
        )
    with tempfile.TemporaryDirectory(prefix="lsp-discovery-") as directory:
        base = Path(directory).resolve()
        home = base / "home"
        home.mkdir()
        script = Path(__file__).resolve()
        # Wrapper names contain spaces to test executable-path handling on both hosts.
        wrapper = base / ("fake server.cmd" if os.name == "nt" else "fake server")
        if os.name == "nt":
            wrapper.write_text(
                f'@echo off\n"{sys.executable}" "{script}" --fake-server\n'
            )
        else:
            wrapper.write_text(
                f"#!/bin/sh\nexec {shlex.quote(sys.executable)} {shlex.quote(str(script))} --fake-server\n"
            )
            wrapper.chmod(0o755)

        def case(name, config=None, files=None, path=None):
            root = base / name
            root.mkdir()
            if config is not None:
                (root / ".lsp-mcp.json").write_text(json.dumps(config))
            for filename, content in (files or {}).items():
                target = root / filename
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content)
            return root, Client(binary, root, home, path)

        root, client = case("no-config")
        try:
            health = client.wait_health("rust - not_detected")
            assert (
                "rust - not_detected" in health and "ruby - not_detected" in health
            ), health
        finally:
            client.close()
        print("PASS no-config default and registry-complete read-only health")

        bindir = base / "server-bin"
        bindir.mkdir()
        default = bindir / ("rust-analyzer.cmd" if os.name == "nt" else "rust-analyzer")
        default.write_bytes(wrapper.read_bytes())
        default.chmod(0o755)
        root, client = case(
            "default-path",
            files={"main.rs": "fn discovery_probe() {}\n"},
            path=str(bindir) + os.pathsep + os.environ.get("PATH", ""),
        )
        try:
            health = client.wait_health("rust - ready")
            assert str(default) in health, health
            assert (root / ".fake-starts").read_text().splitlines() == ["start"]
        finally:
            client.close()
        print("PASS no-config file detection and registry-default PATH discovery")

        config = {"binaries": {"rust": str(wrapper)}, "tools": {"preset": "full"}}
        root, client = case(
            "settings-only", config, {"main.rs": "fn discovery_probe() {}\n"}
        )
        try:
            # First semantic request has no file path and must see background-discovered clients.
            result = client.tool("findSymbol", {"query": "discovery_probe"})
            assert "discovery_probe" in client.text(result), result
            client.wait_health("rust - ready")
            assert (root / ".fake-starts").read_text().splitlines() == ["start"]
        finally:
            client.close()
        print(
            "PASS settings-only auto-selection, executable override, initial workspace query"
        )

        root, client = case("lazy", config)
        try:
            assert "rust - not_detected" in client.health()
            assert not (root / ".fake-starts").exists(), "health must not start LSPs"
            (root / "main.rs").write_text("fn discovery_probe() {}\n")
            args = {
                "requests": json.dumps(
                    [{"path": "main.rs", "line": 1, "character": 4}]
                ),
                "include_definition": False,
            }
            ids = [
                client.begin("tools/call", {"name": "hover", "arguments": args})
                for _ in range(4)
            ]
            for ident in ids:
                result = client.finish(ident)
                assert "discovery-probe-hover" in client.text(result), result
            assert (root / ".fake-starts").read_text().splitlines() == ["start"], (
                "concurrent requests duplicated startup"
            )
            assert "rust - ready" in client.health()
        finally:
            client.close()
        print(
            "PASS late language, concurrent acquisition, warm reuse, hover through MCP"
        )

        for name, selection in [("disabled", []), ("allowlist", ["python"])]:
            cfg = {
                **config,
                "languages": selection,
                "binaries": {
                    **config["binaries"],
                    "python": str(base / "missing-python"),
                },
            }
            root, client = case(name, cfg, {"main.rs": "fn discovery_probe() {}\n"})
            try:
                assert "rust - disabled" in client.health()
                result = client.tool(
                    "hover",
                    {
                        "requests": json.dumps(
                            [{"path": "main.rs", "line": 1, "character": 4}]
                        )
                    },
                )
                assert "disabled" in client.text(result).lower(), result
                assert not (root / ".fake-starts").exists()
            finally:
                client.close()
        print("PASS empty list and explicit allowlist prevent lazy starts")

        root, client = case(
            "missing",
            {"binaries": {"rust": str(base / "installed later")}},
            {"main.rs": "fn discovery_probe() {}\n"},
        )
        try:
            health = client.wait_health("rust - missing_executable")
            assert "installed later" in health and "PATH" in health, health
            later = base / (
                "installed later.cmd" if os.name == "nt" else "installed later"
            )
            later.write_bytes(wrapper.read_bytes())
            later.chmod(0o755)
            result = client.tool(
                "hover",
                {
                    "requests": json.dumps(
                        [{"path": "main.rs", "line": 1, "character": 4}]
                    )
                },
            )
            assert "discovery-probe-hover" in client.text(result), result
            assert "rust - ready" in client.health()
        finally:
            client.close()
        print("PASS actionable missing executable and discovery after installation")

        root, client = case(
            "failed",
            config,
            {"main.rs": "fn discovery_probe() {}\n", ".fake-fail": "yes"},
        )
        try:
            health = client.wait_health("rust - failed")
            assert (
                "fixture initialization refused" in health and str(wrapper) in health
            ), health
        finally:
            client.close()
        print("PASS failed initialization preserves command and error")

        root, client = case(
            "runtime-health", config, {"main.rs": "fn discovery_probe() {}\n"}
        )
        try:
            client.wait_health("rust - ready")
            (root / ".fake-progress").write_text("yes")
            args = {
                "requests": json.dumps(
                    [{"path": "main.rs", "line": 1, "character": 4}]
                ),
                "include_definition": False,
            }
            ident = client.begin("tools/call", {"name": "hover", "arguments": args})
            deadline = time.monotonic() + 10
            while not (root / ".fake-hover-started").exists():
                assert time.monotonic() < deadline
                time.sleep(0.02)
            client.wait_health("rust - initializing")
            assert "discovery-probe-hover" in client.text(client.finish(ident))
            client.wait_health("rust - ready")
            (root / ".fake-progress").unlink()
            (root / ".fake-exit").write_text("yes")
            client.tool("hover", args)
            health = client.wait_health("rust - failed")
            assert "terminated" in health or "exited" in health, health
            result = client.tool("hover", args)
            assert "terminated" in client.text(result) or "exited" in client.text(
                result
            ), result
            assert (root / ".fake-starts").read_text().splitlines() == ["start"]
        finally:
            client.close()
        print("PASS busy-client progress and actionable process-exit health")

        for raw in ['{"languages":["not-a-language"]}', '{"languages":']:
            root = base / ("invalid-" + str(len(raw)))
            root.mkdir()
            (root / ".lsp-mcp.json").write_text(raw)
            env = {**os.environ, "HOME": str(home), "USERPROFILE": str(home)}
            result = subprocess.run(
                [str(binary), "--workspace-root", str(root)],
                capture_output=True,
                check=False,
                env=env,
                timeout=15,
                text=True,
            )
            assert result.returncode != 0 and ".lsp-mcp.json" in result.stderr, result
        print("PASS malformed config and unknown language fail with file context")


if __name__ == "__main__":
    if "--fake-server" in sys.argv:
        fake_server()
    else:
        verify(Path(sys.argv[1]).resolve())
