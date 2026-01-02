 
Use the lspMCP server or native LSP tool for code navigation, symbol lookup, and diagnostics; prefer it over manual search when possible.

We are in active development. There are no breaking changes for MCP responses. It's okay to change the schema.

## Debugging


## Use LSP-MCP to develop
**CRITICAL** This is important. After each task run you must do this.
As you use the lsp-mcp to navigate the codebase to make coding decisions evaluate where the information provided by mcp were not enough and you had to read the file anyways and surface up these shortcomings and insights to the user.
Answer the question if the tool calls and the information provided by the tools helped you navigate the codebase faster and gather required information quicker.
- if you use a tool on a file and then immediately read that file. Explain why you did that

When debug is enabled, logs are written to `.lsp-mcp/logs/sessions/{session-id}.log`. Use the `health` tool to get the current session ID and log file path.