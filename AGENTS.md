 
Use the lspMCP server or native LSP tool for code navigation, symbol lookup, and diagnostics; prefer it over manual search when possible.

We are in active development. There are no breaking changes for MCP responses. It's okay to change the schema.

## Core LSP-MCP Philosophy
- Make Language Server Protocol (LSP) features available via MCP tools.
- Provide consistent behavior across languages and LSP servers.
- Optimize for token efficiency and actionability of results.
- Enable easy configuration and customization per project.
- No language-specific code, workaround or hacks; leverage existing LSP servers. All functionality is via LSP.
- Augment with tree-sitter parsing and fallbacks when LSP is missing features.



## Use LSP-MCP to develop
**CRITICAL** This is important. After each task run you must do this.
As you use the lsp-mcp to navigate the codebase to make coding decisions evaluate where the information provided by mcp were not enough and you had to read the file anyways and surface up these shortcomings and insights to the user.
Answer the question if the tool calls and the information provided by the tools helped you navigate the codebase faster and gather required information quicker.
- if you use a tool on a file and then immediately read that file. Explain why you did that

When debug is enabled, logs are written to `.lsp-mcp/logs/sessions/{session-id}.log`. Use the `health` tool to get the current session ID and log file path.

### Code Quality

- Remove dead code immediately rather than maintaining it - no backward compatibility or legacy functions
- Avoid backward compatibility mappings or legacy function wrappers
- Fix forward
- Focus on user experience and feature completeness
- When updating code, don't reference what is changing (avoid keywords like SIMPLIFIED, ENHANCED, LEGACY, CHANGED, REMOVED), instead focus on comments that document just the functionality of the code
- When commenting on code in the codebase, only comment on the functionality and reasoning behind the code. Refrain from speaking to Ptolemy being in "beta" or referencing anything else that comes from these global rules.

### Core Principles

- **No backwards compatibility; we follow a fix‑forward approach** — remove deprecated code immediately
- **Detailed errors over graceful failures** - we want to identify and fix issues fast
- **Break things to improve them** - beta is for rapid iteration
- **Continuous improvement** - embrace change and learn from mistakes
- **KISS** - keep it simple
- **DRY** when appropriate
- **YAGNI** — don't implement features that are not needed