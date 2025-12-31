Use the lsp-mcp or native LSP tool for code navigation, symbol lookup, and diagnostics; prefer it over manual search when possible.

We are in active development. There are no breaking changes for MCP responses. It's okay to change the schema.

## Debugging
add debug logging to the LSP-MCP code and user can run the mcp server with "RUST_LOG=debug {path-to-compiled-binary} 2>{tmp-log-file}to put the log file to read the raw info the mcp server is getting to diagnose the bug. when the mcp server runs it will write to the log file.

## Use LSP-MCP to develop
**CRITICAL** This is important. After each task run you must do this.
As you use the lsp-mcp to navigate the codebase to make coding decisions evaluate where the information provided by mcp were not enough and you had to read the file anyways and surface up these shortcomings and insights to the user.
Answer the question if the tool calls and the information provided by the tools helped you navigate the codebase faster and gather required information quicker. 
