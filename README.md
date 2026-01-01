# lsp-mcp

LSP-MCP is built off the lsproxy project by Agentic Labs.

The goal is to provide a multi-language MCP server that offers precise code navigation and analysis through Language Servers and ast-grep.

Primarily designed to expose LSP functionality over MCP stdio for AI coding agents. Tested for Golang, Rust, and Typescript. Although support for Python , Ruby etc is also present based on the original lsproxy implementation.


## Key Features

- 🎯 **Precise Cross-File Code Navigation**: Find symbol definitions and references across your entire project.
- 🌐 **Unified MCP Tools**: Access multiple language servers through a single MCP server.
- 🛠️ **Auto-Configuration**: Automatically detect and configure language servers based on your project files.
- 📊 **Code Diagnostics**: (Coming Soon) Get language-specific lint output from an endpoint.
- 🌳 **Call & Type Hierarchies**: (Coming Soon) Query multi-hop code relationships computed by the language servers.
- 🔄 **Procedural Refactoring**: (Coming Soon) Perform symbol operations like `rename`, `extract`, `auto import` through the API.
    
Runs one MCP server per project root. The server uses the current working directory as the workspace unless overridden.

## <a name="installation">Installation</a>
```bash
cargo run --bin lsp-mcp -- --workspace-root /path/to/project
```

Source builds need ast-grep rules installed under `/usr/src/ast_grep`:

```bash
bash scripts/install-ast-grep-rules.sh
```

Configure your MCP client to launch the server with the project root as the working directory. The server communicates over stdio.

## GPU Acceleration (FastEmbed)

FastEmbed uses ONNX Runtime execution providers. GPU acceleration is enabled at build time via cargo features and used
automatically at runtime (highest-priority available provider is selected, then CPU fallback).

Build flags:

```bash
# macOS Apple Silicon (CoreML / ANE where available)
cargo build --features ort-coreml

# Windows NVIDIA (CUDA) + DirectML fallback
cargo build --features ort-cuda,ort-directml

# Windows DirectML only (AMD/Intel/NVIDIA DX12 GPUs)
cargo build --features ort-directml

# Linux AMD ROCm (optional)
cargo build --features ort-rocm
```

Notes:
- CUDA requires a CUDA-enabled ONNX Runtime build and compatible NVIDIA drivers/runtime on the target machine.
- DirectML requires Windows 10+ and a DX12-capable GPU (AMD/Intel/NVIDIA).
- CoreML is supported on macOS and will use ANE when available.

### MCP client configuration

Use a stdio MCP client configuration that launches one server per workspace. The key pieces are the `command` and `args`; if your client supports `cwd`, set it to the workspace root.

Dev (run from source):
```json
{
  "mcpServers": {
    "lsp-mcp": {
      "command": "cargo",
      "args": [
        "run",
        "--bin",
        "lsp-mcp",
        "--",
        "--workspace-root",
        "/path/to/project"
      ],
      "cwd": "/path/to/project"
    }
  }
}
```

Package (installed binary):
```json
{
  "mcpServers": {
    "lsp-mcp": {
      "command": "lsp-mcp",
      "args": [
        "--workspace-root",
        "/path/to/project"
      ],
      "cwd": "/path/to/project"
    }
  }
}
```

Claude Code (`~/.claude.json`):
Dev (run from source):
```json
{
  "mcpServers": {
    "lsp-mcp": {
      "type": "stdio",
      "command": "cargo",
      "args": [
        "run",
        "--bin",
        "lsp-mcp",
        "--",
        "--workspace-root",
        "/path/to/project"
      ],
      "cwd": "/path/to/project"
    }
  }
}
```

Package (installed binary):
```json
{
  "mcpServers": {
    "lsp-mcp": {
      "type": "stdio",
      "command": "lsp-mcp",
      "args": [
        "--workspace-root",
        "/path/to/project"
      ],
      "cwd": "/path/to/project"
    }
  }
}
```

Codex CLI (`~/.codex/config.toml`):
Dev (run from source):
```toml
[mcp_servers.lsp-mcp]
command = "cargo"
args = ["run", "--bin", "lsp-mcp", "--", "--workspace-root", "/path/to/project"]
```

Package (installed binary):
```toml
[mcp_servers.lsp-mcp]
command = "lsp-mcp"
args = ["--workspace-root", "/path/to/project"]
```

## Configuration

You can customize `lsp-mcp` behavior with a `.lsp-mcp.json` file in your workspace root.
See `CONFIGURATION.md` for the full schema and examples.

### First-time setup

On first launch (no project config), the `initialSetup` tool is enabled in the standard preset to help configure languages and binaries.
If a project `.lsp-mcp.json` exists, `initialSetup` is auto-disabled unless you set `"tools": { "initial_setup": "enabled" }`.
After setup, add `initialSetup` to `"tools": { "disable": ["initialSetup"] }` and restart your agent.

### Output Mode

Control the verbosity of tool outputs:

```json
{
  "output": {
    "mode": "verbose"
  }
}
```

- `"default"` (default): Compact output format
- `"verbose"`: Detailed output with full information

## <a name="contributing">Building products with lsproxy</a>

If you're building AI coding agents or code RAG, or would like to use `lsproxy` in a commercial product, please reach out!

## <a name="contributing">Contributing</a>

We appreciate all contributions! You don't need to be an expert to help out.
Please see [CONTRIBUTING.md](https://github.com/agentic-labs/lsproxy/blob/main/CONTRIBUTING.md) for more details on how to get
started.

> Questions? Reach out to us [on Discord](https://discord.gg/WafeS3jN).

## <a name="community">Community</a>

We're building a community. Come hang out with us!

- 🌟 [Star us on GitHub](https://github.com/agentic-labs/lsproxy)
- 💬 [Chat with us on Discord](https://discord.gg/EUFGjSawyk)
- ✏️ [Start a GitHub Discussion](https://github.com/agentic-labs/lsproxy/discussions)
- 🐦 [Follow us on Twitter](https://twitter.com/agentic_labs)
- 🕴️ [Follow us on LinkedIn](https://www.linkedin.com/company/agentic-labs)
  
## <a name="supported-languages">Supported languages</a>

We're looking to add new language support or better language servers so let us know what you need!
|Language|Server|URL|
|:-|:-|:-|
|C/C++|`clangd`|https://clangd.llvm.org/|
|Golang|`gopls`|https://github.com/golang/tools/tree/master/gopls|
|Java|`jdtls`|https://github.com/eclipse-jdtls/eclipse.jdt.ls|
|Javascript|`typescript-language-server`|https://github.com/typescript-language-server/typescript-language-server|
|PHP|`phpactor`|https://github.com/phpactor/phpactor|
|Python|`jedi-language-server`|https://github.com/pappasam/jedi-language-server|
|Rust|`rust-analyzer`|https://github.com/rust-lang/rust-analyzer|
|Typescript|`typescript-language-server`|https://github.com/typescript-language-server/typescript-language-server|
|Your Favorite Language | Awesome Language Server | https://github.com/agentic-labs/lsproxy/issues/new |
