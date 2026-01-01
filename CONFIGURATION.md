Configuration
=============

This file describes the `.lsp-mcp.json` configuration format and merge rules.

Locations
---------

- Project config: `.lsp-mcp.json` in the workspace root.
- Global config: `~/.lsp-mcp/.lsp-mcp.json`.

Merge behavior
--------------

- Project config overrides global config for `languages`, `binaries`, and `tools`.
- `output` only overrides when it is explicitly set in the project config.
- When `output` is not present in the project config, the global `output` mode applies.

Settings
--------

Output mode:
- `output.mode` values: `default` (compact JSON, no meta) or `verbose` (pretty JSON + meta).
- Default: `default`.
- No per-tool override.

Tools:
- `tools.preset`: `minimal`, `standard`, or `full`.
- `tools.enable`: list of tool names to enable in addition to the preset.
- `tools.disable`: list of tool names to disable (takes precedence).
- `tools.initial_setup`: `auto` (default), `enabled`, or `disabled`.
  - `auto`: enable `initialSetup` only when the preset is `standard` and no project config exists.
  - `enabled`: always enable `initialSetup`.
  - `disabled`: always disable `initialSetup`.

Languages:
- `languages`: list of language identifiers to start language servers for.

Binaries:
- `binaries`: map of language identifier to language server path.

Examples
--------

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
    "enable": ["findReferencedSymbols"],
    "disable": ["incoming_calls"],
    "initial_setup": "auto"
  },
  "output": {
    "mode": "default"
  }
}
```
