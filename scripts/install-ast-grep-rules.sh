# ABOUTME: Installs ast-grep rules used by lsp-mcp builds.
# ABOUTME: Copies bundled rules from src/ast_grep to system location.
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
local_rules="${repo_root}/src/ast_grep"

if [[ ! -d "$local_rules" ]]; then
  echo "Local ast-grep rules not found at ${local_rules}" >&2
  exit 1
fi

os_name="$(uname -s)"
default_root="/usr/src"
if [[ "$os_name" == "Darwin" ]]; then
  if [[ -d "/opt/homebrew" ]]; then
    default_root="/opt/homebrew/share"
  else
    default_root="/usr/local/share"
  fi
fi

install_target="${AST_GREP_RULES_DIR:-${default_root}/ast_grep}"

# Helper function to run commands with or without sudo
run_cmd() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

# Create install directory
run_cmd mkdir -p "$install_target"

# Copy main rules from src/ast_grep
for ruleset in identifier reference symbol; do
  if [[ -d "${local_rules}/${ruleset}" ]]; then
    run_cmd mkdir -p "${install_target}/${ruleset}"
    run_cmd cp -R "${local_rules}/${ruleset}/." "${install_target}/${ruleset}/"
  fi
done

# Copy custom reference rules if they exist (from ast_grep/reference/rules)
custom_reference_rules="${repo_root}/ast_grep/reference/rules"
if [[ -d "$custom_reference_rules" ]]; then
  run_cmd mkdir -p "${install_target}/reference/rules"
  run_cmd cp -R "${custom_reference_rules}/." "${install_target}/reference/rules/"
fi

# Create symlink on Linux if not installing to /usr/src
if [[ "$os_name" != "Darwin" && "$(dirname "$install_target")" != "/usr/src" ]]; then
  run_cmd mkdir -p /usr/src
  run_cmd ln -sfn "$install_target" /usr/src/ast_grep
fi

echo "ast-grep rules installed under ${install_target}"
if [[ "$os_name" == "Darwin" ]]; then
  echo "Set AST_GREP_RULES_DIR=${install_target} when running lsp-mcp if not using the default."
fi
