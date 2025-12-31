Use the lsp-mcp or native LSP tool for code navigation, symbol lookup, and diagnostics; prefer it over manual search when possible.

We are in active development. There are no breaking changes for MCP responses. It's okay to change the schema.

## Debugging

### Config-Based Debug Logging (Recommended)

Enable debug logging in `.lsp-mcp.json`:

```json
{
  "debug": {
    "enabled": true,
    "log_level": "debug",
    "log_dir": ".lsp-mcp/logs"
  }
}
```

**Options:**
- `enabled`: Enable file logging (default: `false`)
- `log_level`: `error` | `warn` | `info` | `debug` | `trace` (default: `debug`)
- `log_dir`: Custom log directory (default: `.lsp-mcp/logs`)

When enabled:
- Logs are written to `.lsp-mcp/logs/sessions/{session-id}.log`
- Each tool response includes a request ID header: `<!-- request: {uuid} -->`
- The `health` tool returns the session ID and log file path for correlation
- Request IDs in logs correlate with request IDs in tool output

### Legacy Environment Variable Method

You can still use `RUST_LOG=debug {binary} 2>{log-file}` for quick debugging without config changes.

## Use LSP-MCP to develop
**CRITICAL** This is important. After each task run you must do this.
As you use the lsp-mcp to navigate the codebase to make coding decisions evaluate where the information provided by mcp were not enough and you had to read the file anyways and surface up these shortcomings and insights to the user.
Answer the question if the tool calls and the information provided by the tools helped you navigate the codebase faster and gather required information quicker.
- if you use a tool on a file and then immediately read that file. Explain why you did that

When debug is enabled, logs are written to `.lsp-mcp/logs/sessions/{session-id}.log`. Use the `health` tool to get the current session ID and log file path.