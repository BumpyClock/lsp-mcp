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

url="https://github.com/agentic-labs/lsproxy/releases/download/${version}/lsproxy-${version}-ast-grep-rules.tar.gz"
curl -fsSL -o "$archive" "$url"

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
