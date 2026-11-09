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

The server watches workspace files for changes. If native file watching is unavailable, it logs a warning and polls every two seconds. File events are debounced for two seconds. The same fallback keeps the semantic search index current. Watchers stop when their owning service stops. If both backends fail, startup reports the workspace path and both errors.

Language health reports `initializing` while the server reports standard LSP work in progress, including workspace loading and indexing. It reports `ready` when that work finishes. Servers that do not report work progress become ready after initialization.

## Pre-building the Semantic Search Index

When semantic search is enabled, the initial indexing can be resource-intensive. To avoid slowdowns during normal MCP usage, you can pre-build the index from the command line:

```bash
# Build index for current directory
lsp-mcp --index

# Build index for a specific workspace
lsp-mcp --index --workspace-root /path/to/project

# Force full rebuild from scratch
lsp-mcp --index --force
```

The `--index` flag:
- Builds the semantic search index and exits (does not start the MCP server)
- Auto-enables semantic search even if not configured in `.lsp-mcp.json`
- Shows progress: `Indexing: 150/500 files (30%)`
- Reports final stats: chunks indexed and time elapsed

By default, indexes are stored in `~/.lsp-mcp/index/<project-name>-<hash>` using the canonical workspace path. Symlink aliases share an index. Separate clones and worktrees use separate indexes. Moving a workspace may create a new index, and existing workspace-local indexes are not migrated automatically. Set `semantic_search.vector_store.path` to choose a different location. Relative paths are resolved against the workspace root.

The `--force` flag deletes any existing index and rebuilds from scratch.

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

When `languages` is omitted, LSP-MCP scans the workspace with gitignore rules to detect supported languages. An explicit list is an allowlist, and `[]` disables all language servers. Project settings override global settings. Unknown languages and configuration parse errors fail startup.

Servers start in the background for the workspace root. File requests can start a server lazily when a language appears later. Binary overrides take precedence over registry defaults; default discovery searches PATH only and does not install servers or discover project-local binaries. Missing executables are retried on later requests. The read-only `health` tool reports selection and server status, including missing executables and failures. Detection does not create additional monorepo roots or add arbitrary language support.

### First-time setup

On first launch (no project config), the `initialSetup` tool is enabled in the standard preset for optional language and binary overrides. `health` is also available in the standard preset.

Verify automatic discovery against a built or installed binary with `python scripts/verify_discovery.py /path/to/lsp-mcp`. The probe uses fixture workspaces and a fake LSP to check automatic and lazy startup, PATH resolution, allowlists, concurrent requests, live health, and configuration errors.
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
|csharp|`omnisharp-roslyn`|https://github.com/OmniSharp/omnisharp-roslyn|
|Your Favorite Language | Awesome Language Server | https://github.com/agentic-labs/lsproxy/issues/new |
