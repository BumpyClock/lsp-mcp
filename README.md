# lsp-mcp

LSP-MCP is built off the lsproxy project by Agentic Labs.

The goal is to provide a multi-language MCP server that offers precise code navigation and analysis through Language Servers and tree-sitter

Primarily designed to expose LSP functionality over MCP stdio for AI coding agents. Tested for Golang, Rust, and Typescript. Although support for Python , Ruby etc is also present based on the original lsproxy implementation.

## Credits
This is built on top of the lsproxy project by Agentic Labs and tree-sitter implementation is inspired by the Roo Code project. All credits to them for the original implementation.

## Key Features

- 🎯 **Precise Code Navigation**: Jump to definitions, find all references, and navigate implementations across your entire project.
- 🔍 **Symbol Search**: Search symbols by name across the workspace or find identifiers within files.
- 📝 **Code Intelligence**: Get hover information, type signatures, and context-aware code details at any position.
- 🌳 **Call Hierarchies**: Query incoming and outgoing function calls, including external dependencies.
- 🔗 **Reference Analysis**: Find symbols referenced by a definition and explore code relationships.
- 📊 **Code Diagnostics**: Get language-specific lint errors, warnings, and diagnostics for files or the entire workspace.
- 🔄 **Semantic Search**: (Optional) Perform natural language code search using embeddings for semantic similarity.
- 🌐 **Multi-Language Support**: Access multiple language servers (Rust, TypeScript, Go, Python, Ruby, Java, PHP, C/C++) through a single MCP server.
- 🛠️ **Auto-Configuration**: Guided setup tool to detect and configure language servers based on your project files.
- 📁 **File Operations**: List workspace files and read source code with line/character range support.
    
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

***Note on CoreML:*** I've tested this on my M1 Max Macbook Pro and it works but is slower than the CPU fallback. CPU fallback will consume a lot of resources on initial indexing for a couple of minutes and then settle down to minor spikes when re-indexing files. 

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

  
## <a name="supported-languages">Supported languages</a>

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
