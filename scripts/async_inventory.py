#!/usr/bin/env python3
"""List Rust await sites by seam for a repeatable source audit.

Requires ast-grep. This inventories syntax, including tests, rather than proving
that operations are independent or that a path is hot at runtime.
Await syntax inside macro token trees, such as tokio::select!, requires manual review.
"""

import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit file and await-site inventory")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    result = subprocess.run(
        ["ast-grep", "run", "--lang", "rust", "--pattern", "$EXPR.await", "--json=compact", "src"],
        cwd=root, capture_output=True, text=True,
    )
    if result.returncode not in (0, 1):
        raise SystemExit(result.stderr)
    matches = json.loads(result.stdout or "[]")
    files = {str(path.relative_to(root)): [] for path in sorted((root / "src").rglob("*.rs"))}
    for match in matches:
        files[match["file"]].append({
            "line": match["range"]["start"]["line"] + 1,
            "expression": match["text"],
        })
    seams = {"lsp": {}, "index": {}, "api": {}}
    for path, awaits in files.items():
        seam = "lsp" if path.startswith("src/lsp/") else (
            "index" if path.startswith(("src/semantic_search/", "src/codemap/")) else "api"
        )
        seams[seam][path] = sorted(awaits, key=lambda site: site["line"])
    if args.json:
        print(json.dumps(seams, indent=2))
    else:
        for seam, entries in seams.items():
            print(f"{seam}: {len(entries)} Rust files, {sum(map(len, entries.values()))} AST await expressions (includes tests; excludes macro token trees)")
            for path, awaits in entries.items():
                print(f"  {path}: {len(awaits)}")


if __name__ == "__main__":
    main()
