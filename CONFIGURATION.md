# Configuration

This file describes the `.lsp-mcp.json` configuration format and merge rules.

## Index

- [Locations](#locations)
- [Merge Behavior](#merge-behavior)
- [Settings](#settings)
  - [Output](#output)
  - [Tools](#tools)
  - [Debug Logging](#debug-logging)
  - [Languages](#languages)
  - [Binaries](#binaries)
  - [Semantic Search](#semantic-search)
- [Examples](#examples)

## Locations

- Project config: `.lsp-mcp.json` in the workspace root.
- Global config: `~/.lsp-mcp/.lsp-mcp.json`.

## Merge Behavior

- Project config overrides global config for `languages`.
- `binaries` merge per key (project values override matching global keys).
- `tools` merge per field; `enable`/`disable` lists are unioned when both are present.
- `output`, `debug`, and `semantic_search` merge field-by-field.
- For nested `semantic_search` sections (like `embedder`, `vector_store`, `exclude`, `enrichment`), fields only override when present in the project config.

## Settings

### Output

- `output.mode` values: `default` (compact JSON, no meta) or `verbose` (pretty JSON + meta).
- Default: `default`.
- No per-tool override.

### Tools

- `tools.preset`: `minimal`, `standard`, or `full`.
- `health` is included in the default `standard` preset.
- `tools.enable`: list of tool names to enable in addition to the preset.
- `tools.disable`: list of tool names to disable (takes precedence).
- `tools.initial_setup`: `auto` (default), `enabled`, or `disabled`.
  - `auto`: enable `initialSetup` only when the preset is `standard` and no project config exists.
  - `enabled`: always enable `initialSetup`.
  - `disabled`: always disable `initialSetup`.

### Debug Logging

- `debug.enabled`: enable file logging (default: `false`)
- `debug.log_level`: `error` | `warn` | `info` | `debug` | `trace` (default: `debug`)
- `debug.log_dir`: log directory (default: `.lsp-mcp/logs`)

When enabled:
- Logs are written to `.lsp-mcp/logs/sessions/{session-id}.log`
- Each tool response includes a request ID header: `<!-- request: {uuid} -->`
- The `health` tool returns the session ID and log file path for correlation

### Languages

- Omit `languages` to detect supported languages by scanning the workspace while respecting gitignore rules. A project list overrides the global list. If the project omits it, the global setting applies.
- An explicit `languages` list is an allowlist. `[]` disables all language servers.
- Unknown language identifiers and configuration parse errors fail startup.
- Clients use the configured workspace root. Detection does not create additional monorepo roots or add arbitrary language support.
- Detected servers start in the background. File requests can start servers lazily for newly introduced languages. A missing executable is retried on a later request.
- The read-only `health` tool reports selection, missing executable, failure, initialization, and ready details without starting servers.

### Binaries

- `binaries`: map of language identifier to language server path. An override takes precedence over the registry default. Default executable discovery searches PATH only. It does not install servers or discover and run project-local binaries.
  Unavailable-server errors name the default executable and retain startup failure details. Install the server if missing, check PATH or configure its binary path, and inspect startup logs if it is already installed.

For TypeScript/JavaScript, `typescript-language-server` also requires a TypeScript
installation that provides `tsserver.js`.

Hover batches return an MCP error when every item fails. Mixed batches retain
successful results and individual errors in request order.

### Semantic Search

Note: `semantic_search.index` and `semantic_search.search` are removed; all options now live directly under `semantic_search`.

- `semantic_search.enabled`: enable semantic search (default: `false`).
- `semantic_search.embedder`:
  - `provider`: `fastembed` (default) or `openai`.
  - `fastembed` options:
    - `model` (default: `BAAI/bge-small-en-v1.5`)
    - `dimension` (default: `384`, must match the selected model; validated at startup; mismatches are rejected; index rebuilds on dimension changes)
    - `cache_dir` (default: `~/.lsp-mcp/.fastembed-cache`, or `FASTEMBED_CACHE_DIR` if set)
  - `openai` options:
    - `model` (default: `text-embedding-3-small`)
    - `base_url` (default: `https://api.openai.com/v1`)
    - `api_key` (optional; prefer `api_key_env`)
    - `api_key_env` (default: `OPENAI_API_KEY`)
    - `dimension` (default: `1536`)
- `semantic_search.vector_store`:
  - `path`: optional storage path. Relative paths are resolved against the workspace root; absolute paths are used directly. When omitted, the index is stored under `~/.lsp-mcp/index/<project-name>-<hash>`, where the hash identifies the canonical absolute workspace path. Symlink aliases share an index; separate clones and worktrees have separate indexes. Moving a workspace may create a new index. Existing workspace-local indexes are not migrated automatically.
- `semantic_search.include`: list of glob patterns to include
- `semantic_search.exclude`:
  - `files`: list of glob patterns to exclude files
  - `directories`: list of glob patterns to exclude directories
- `semantic_search.max_file_size_mb`: max file size in MB (default: `1`)
- `semantic_search.min_chunk_chars`: minimum chunk size (default: `50`)
- `semantic_search.max_chunk_chars`: maximum chunk size (default: `2000`)
- `semantic_search.max_function_chunk_chars`: maximum chunk size for functions (default: `5000`)
- `semantic_search.chunk_overlap_chars`: overlap size between chunks (default: `200`)
- `semantic_search.batch_size`: embedding batch size (default: `60`)
- `semantic_search.respect_gitignore`: whether to skip files in .gitignore (default: `true`)
- `semantic_search.min_score`: minimum similarity (default: `0.4`)
- `semantic_search.max_results`: maximum results (default: `5`)
- `semantic_search.default_context_lines`: max lines per result chunk (default: `15`, `null` = full chunk)

Note: Semantic search requires both `semantic_search.enabled: true` and `tools.enable` to include `semanticSearch`.

## Examples

verbose output mode:
```json
{
  "output": {
    "mode": "verbose"
  }
}
```

Full example:
```json
{
  "languages": ["rust", "typescript"],
  "binaries": {
    "rust": "/opt/rust-analyzer"
  },
  "tools": {
    "preset": "standard",
    "enable": ["findReferencedSymbols", "semanticSearch"],
    "disable": ["incoming_calls"],
    "initial_setup": "auto"
  },
  "debug": {
    "enabled": true,
    "log_level": "debug",
    "log_dir": ".lsp-mcp/logs"
  },
  "semantic_search": {
    "enabled": true,
    "embedder": {
      "provider": "fastembed"
    },
    "exclude": {
      "directories": ["**/node_modules/**"]
    },
    "respect_gitignore": false,
    "default_context_lines": 15
  },
  "output": {
    "mode": "default"
  }
}
```
