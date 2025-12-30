# ABOUTME: Installs ast-grep rules used by lsproxy source builds.
# ABOUTME: Downloads release rules and extracts them into /usr/src/ast_grep.
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version_line="$(grep -m 1 '^version = ' "$repo_root/lsproxy/Cargo.toml" || true)"

if [[ -z "$version_line" ]]; then
  echo "Failed to detect lsproxy version from lsproxy/Cargo.toml" >&2
  exit 1
fi

version="$(printf '%s' "$version_line" | sed -E 's/.*\"([^\"]+)\".*/\1/')"

if [[ -z "$version" ]]; then
  echo "Failed to parse lsproxy version from lsproxy/Cargo.toml" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required to download ast-grep rules" >&2
  exit 1
fi

archive="$(mktemp -t lsproxy-ast-grep-XXXXXX.tar.gz)"
trap 'rm -f "$archive"' EXIT

rules_url="${AST_GREP_RULES_URL:-}"
if [[ -z "$rules_url" ]]; then
  rules_version="${AST_GREP_RULES_VERSION:-$version}"
  rules_url="https://github.com/agentic-labs/lsproxy/releases/download/${rules_version}/lsproxy-${rules_version}-ast-grep-rules.tar.gz"
fi

if ! curl -fsSL -o "$archive" "$rules_url"; then
  if [[ -n "${AST_GREP_RULES_URL:-}" || -n "${AST_GREP_RULES_VERSION:-}" ]]; then
    echo "Failed to download ast-grep rules from ${rules_url}" >&2
    exit 1
  fi

  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required to resolve the latest ast-grep rules release" >&2
    exit 1
  fi

  latest_url="$(python3 - <<'PY'
import json
import urllib.request

url = "https://api.github.com/repos/agentic-labs/lsproxy/releases/latest"
with urllib.request.urlopen(url) as response:
    data = json.load(response)

assets = data.get("assets", [])
for asset in assets:
    name = asset.get("name", "")
    if "ast-grep-rules" in name:
        print(asset.get("browser_download_url", ""))
        break
PY
)"

  if [[ -z "$latest_url" ]]; then
    echo "Failed to resolve latest ast-grep rules download URL from GitHub" >&2
    exit 1
  fi

  curl -fsSL -o "$archive" "$latest_url"
fi

if [[ "$(id -u)" -eq 0 ]]; then
  mkdir -p /usr/src
  tar -xzf "$archive" -C /usr/src --no-same-owner
else
  sudo mkdir -p /usr/src
  sudo tar -xzf "$archive" -C /usr/src --no-same-owner
fi

custom_reference_rules="${repo_root}/ast_grep/reference/rules"
if [[ -d "$custom_reference_rules" ]]; then
  if [[ "$(id -u)" -eq 0 ]]; then
    mkdir -p /usr/src/ast_grep/reference/rules
    cp -R "${custom_reference_rules}/." /usr/src/ast_grep/reference/rules/
  else
    sudo mkdir -p /usr/src/ast_grep/reference/rules
    sudo cp -R "${custom_reference_rules}/." /usr/src/ast_grep/reference/rules/
  fi
fi

echo "ast-grep rules installed under /usr/src/ast_grep"
