#!/usr/bin/env python3
"""Exercise discovery and concurrent navigation through a built MCP binary.

Usage: python3 scripts/verify_navigation.py /path/to/lsp-mcp [--discovery-only]
Uses a deterministic language server. No network or installed LSP required.
"""

import json
import os
import queue
import shlex
import sys
import tempfile
import time
from pathlib import Path

from verify_discovery import Client, lsp_read, lsp_send


def fake_server():
    root = Path.cwd()
    while True:
        message = lsp_read()
        if message is None:
            return
        with (root / ".requests").open("a") as log:
            log.write(json.dumps(message) + "\n")
        method = message.get("method")
        if "id" not in message:
            if method == "$/cancelRequest":
                (root / ".cancelled").write_text(str(message["params"]["id"]))
            continue
        result = None
        if method == "initialize":
            result = {"capabilities": {"hoverProvider": True, "documentSymbolProvider": True, "workspaceSymbolProvider": True}}
        elif method == "textDocument/documentSymbol":
            def symbol(name, kind, line, detail, children=None):
                position = {"line": line, "character": 7}
                return {"name": name, "kind": kind, "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 35}}, "selectionRange": {"start": position, "end": {"line": line, "character": 15}}, "detail": detail, "children": children or []}
            result = [symbol("Target", 23, 0, "struct Target", [symbol("value", 8, 0, "value: i32")])]
            result += [symbol("target_" + str(i), 12, i + 1, "fn target_" + str(i) + "() -> i32") for i in range(8)]
        elif method == "workspace/symbol":
            result = [{"name": "target_" + str(i), "kind": 12, "location": {"uri": (root / "src/lib.rs").as_uri(), "range": {"start": {"line": i + 1, "character": 7}, "end": {"line": i + 1, "character": 15}}}} for i in range(8)]
            if (root / ".reexport-first").exists():
                alias = {**result[0], "location": {**result[0]["location"]}}
                alias["location"]["range"] = {"start": {"line": 9, "character": 9}, "end": {"line": 9, "character": 17}}
                result.insert(0, alias)
        elif method == "textDocument/hover":
            if (root / ".hold-hover").exists():
                (root / ".hover-started").write_text(str(message["id"]))
                continue
            result = {"contents": {"kind": "markdown", "value": "```rust\nstruct Unrelated\n```\nUnrelated hover documentation."}}
        elif method == "textDocument/definition":
            result = []
        lsp_send({"jsonrpc": "2.0", "id": message["id"], "result": result})


def finish_within(client, ident, seconds):
    deadline = time.monotonic() + seconds
    while ident not in client.pending:
        try:
            item = client.messages.get(timeout=max(0.001, deadline - time.monotonic()))
        except queue.Empty as error:
            raise AssertionError("independent outline must not wait for held hover") from error
        assert "eof" not in item and "invalid_stdout" not in item, item
        if "id" in item:
            client.pending[item["id"]] = item
    response = client.pending.pop(ident)
    assert "error" not in response, response
    return response["result"]


def verify(binary, discovery_only):
    with tempfile.TemporaryDirectory(prefix="lsp-navigation-") as directory:
        base = Path(directory).resolve()
        home = base / "home"
        home.mkdir()
        wrapper = base / ("fake server.cmd" if os.name == "nt" else "fake server")
        script = Path(__file__).resolve()
        if os.name == "nt":
            wrapper.write_text(f'@echo off\n"{sys.executable}" "{script}" --fake-server\n')
        else:
            wrapper.write_text(f"#!/bin/sh\nexec {shlex.quote(sys.executable)} {shlex.quote(str(script))} --fake-server\n")
            wrapper.chmod(0o755)
        root = base / "workspace"
        (root / "src").mkdir(parents=True)
        source = "pub struct Target { pub value: i32 }\n" + "".join(f"pub fn target_{i}() -> i32 {{ {i} }}\n" for i in range(8))
        source += "export { target_0 } from './other';\n"
        (root / "src/lib.rs").write_text(source)
        (root / "Cargo.toml").write_text('[package]\nname="navigation"\nversion="0.1.0"\nedition="2021"\n')
        (root / ".lsp-mcp.json").write_text(json.dumps({"languages": ["rust"], "binaries": {"rust": str(wrapper)}, "tools": {"preset": "full"}, "semantic_search": {"enabled": False}}))
        client = Client(binary, root, home)
        try:
            client.wait_health("rust - ready")
            tools = client.request("tools/list", {})["tools"]
            for name in ["health", "documentSymbol", "findSymbol", "getSymbolDefinition", "hover", "findReferences"]:
                tool = next(tool for tool in tools if tool["name"] == name)
                assert tool["annotations"]["readOnlyHint"] is True, tool
                assert tool["annotations"]["destructiveHint"] is False, tool
            outline = client.tool("documentSymbol", {"path": "src/lib.rs", "limit": 1, "context_lines": 0})
            assert not outline.get("isError"), outline
            requests = [json.loads(line) for line in (root / ".requests").read_text().splitlines()]
            methods = [r.get("method") for r in requests]
            assert methods.count("textDocument/documentSymbol") == 1, methods
            assert "textDocument/hover" not in methods and "textDocument/definition" not in methods, methods
            text = client.text(outline)
            assert "Target" in text and "struct Target" in text and "Unrelated" not in text, text
            assert "value (" not in text and "target_0" not in text, text
            print("PASS paginated outline preserves declaration detail without hidden enrichment", flush=True)

            nested = client.text(client.tool("documentSymbol", {"path": "src/lib.rs", "include_children": True, "limit": 1, "context_lines": 0}))
            assert "value" in nested and "target_0" not in nested, nested
            search = client.text(client.tool("findSymbol", {"query": "target", "limit": 1, "context_lines": 0}))
            assert "target_0" in search and "target_1" not in search, search
            methods = [json.loads(line).get("method") for line in (root / ".requests").read_text().splitlines()]
            assert "textDocument/hover" not in methods and "textDocument/definition" not in methods, methods
            print("PASS nested outline and symbol search remain navigable without hover fan-out", flush=True)

            definition = client.text(client.tool("getSymbolDefinition", {"symbol_name": "target_0", "file_path": "src/lib.rs"}))
            assert "pub fn target_0() -> i32 { 0 }" in definition, definition
            print("PASS follow-up definition returns actual source body", flush=True)

            (root / ".reexport-first").touch()
            primary = client.text(client.tool("findSymbol", {"query": "target_0", "exact": True, "limit": 1, "context_lines": 0}))
            assert "src/lib.rs:2" in primary and "re-export" not in primary, primary
            print("PASS declaration wins over an earlier unclassified re-export", flush=True)
            (root / ".reexport-first").unlink()

            if discovery_only:
                return
            (root / ".hold-hover").touch()
            hover = client.begin("tools/call", {"name": "hover", "arguments": {"requests": json.dumps([{"path": "src/lib.rs", "line": 1, "character": 12}]), "include_definition": False}})
            deadline = time.monotonic() + 3
            while not (root / ".hover-started").exists():
                assert time.monotonic() < deadline, "hover must reach server"
                time.sleep(0.01)
            outline_id = client.begin("tools/call", {"name": "documentSymbol", "arguments": {"path": "src/lib.rs", "limit": 1, "context_lines": 0}})
            independent = finish_within(client, outline_id, 2)
            assert not independent.get("isError") and "Target" in client.text(independent), independent
            print("PASS independent outline completes while hover remains unanswered", flush=True)
            timeout = client.finish(hover)
            assert timeout.get("isError"), timeout
            assert "timed out" in client.text(timeout).lower(), timeout
            deadline = time.monotonic() + 2
            while not (root / ".cancelled").exists():
                assert time.monotonic() < deadline, "timeout must cancel server work"
                time.sleep(0.01)
            assert (root / ".cancelled").read_text() == (root / ".hover-started").read_text()
            (root / ".hold-hover").unlink()
            resumed = client.tool("hover", {"requests": json.dumps([{"path": "src/lib.rs", "line": 1, "character": 12}]), "include_definition": False})
            assert not resumed.get("isError") and "Unrelated hover documentation" in client.text(resumed), resumed
            print("PASS timed-out request cancels on wire, reports error, and leaves client usable", flush=True)
        finally:
            client.close()


if __name__ == "__main__":
    if "--fake-server" in sys.argv:
        fake_server()
    else:
        verify(Path(sys.argv[1]).resolve(), "--discovery-only" in sys.argv)
