<div align="center">
<a href="https://agenticlabs.com/"><img src="https://raw.githubusercontent.com/agentic-labs/.github/main/assets/logo.png" alt="Agentic Labs" title="Agentic Labs" align="center" height="150px" /></a>

# lsproxy - Precise code navigation via MCP
<p align="center">
  <a href="https://discord.gg/EUFGjSawyk"a><img alt="discord" src="https://img.shields.io/discord/1296271531994775552" /></a>
  <img alt="license" src="https://img.shields.io/github/license/agentic-labs/lsproxy" />
</p>
</div>


   
## <a name="what-is-lsproxy">What is lsproxy?</a>

`lsproxy` offers IDE-like code analysis and navigation through an MCP stdio server.

It supports [multiple languages](#supported-languages) and resolves relationships between code symbols (functions, classes, variables) anywhere in the project - which can be used to help AI assistants navigate a codebase or build custom code RAG systems.

`lsproxy` runs [Language Servers](https://microsoft.github.io/language-server-protocol/) and [ast-grep](https://github.com/ast-grep/ast-grep) under the hood, giving you precise search results without the headache of configuring and integrating language-specific tooling.

[![](https://mermaid.ink/img/pako:eNptUtFumzAU_RV0q0qdRKpAgAAPk6buZVInTau0h9ZV5YRrYhVsZJuuLMq_7xraNLQ1D9jnnHt8ru09bHWFUIJo9N_tjhsXXP9miqmAhqfuLhhc0X_DLTL4cu-5ibX9pja825FMOS4VmjsGje2Mfh4Y3E8iP55007ej0Z9xNtm8slRBdddc1T2vMbhB84TGzgx4TQpu3aI22M2ZThL17dePTzYMFouv3v1TnNezBBPWydM95xiq6tj40D5I9SBkg75jaZ2HNrqxgVSBh49pKhSWNEKq6kXjIamkk1odVeajiiA0qLY4ncTpjVCugMFP3SvHYAw59TUpKPCInYScEz7SHDEjMmHn54F1Q4Nvl-obasozzMRKiNA6ox-xPEt4scT4Xc1O01FMcpH6771nI1E5-yYRIoUQWjQtlxU9wr0vYOB26F9JSdOKm0cGTB1Ix3unbwa1hdKZHkPou4o7_C45PcMWSsEbS2jH1a3W7auIllDu4RnKJLtM0yJL82hdJKs4zUIYoIyj5WWeJlGyzKNslefr5BDCv9GAiCKOi6yIlnGeFkmxPvwHnPP5bQ?type=png)](https://mermaid.live/edit#pako:eNptUtFumzAU_RV0q0qdRKpAgAAPk6buZVInTau0h9ZV5YRrYhVsZJuuLMq_7xraNLQ1D9jnnHt8ru09bHWFUIJo9N_tjhsXXP9miqmAhqfuLhhc0X_DLTL4cu-5ibX9pja825FMOS4VmjsGje2Mfh4Y3E8iP55007ej0Z9xNtm8slRBdddc1T2vMbhB84TGzgx4TQpu3aI22M2ZThL17dePTzYMFouv3v1TnNezBBPWydM95xiq6tj40D5I9SBkg75jaZ2HNrqxgVSBh49pKhSWNEKq6kXjIamkk1odVeajiiA0qLY4ncTpjVCugMFP3SvHYAw59TUpKPCInYScEz7SHDEjMmHn54F1Q4Nvl-obasozzMRKiNA6ox-xPEt4scT4Xc1O01FMcpH6771nI1E5-yYRIoUQWjQtlxU9wr0vYOB26F9JSdOKm0cGTB1Ix3unbwa1hdKZHkPou4o7_C45PcMWSsEbS2jH1a3W7auIllDu4RnKJLtM0yJL82hdJKs4zUIYoIyj5WWeJlGyzKNslefr5BDCv9GAiCKOi6yIlnGeFkmxPvwHnPP5bQ)

## Key Features

- 🎯 **Precise Cross-File Code Navigation**: Find symbol definitions and references across your entire project.
- 🌐 **Unified MCP Tools**: Access multiple language servers through a single MCP server.
- 🛠️ **Auto-Configuration**: Automatically detect and configure language servers based on your project files.
- 📊 **Code Diagnostics**: (Coming Soon) Get language-specific lint output from an endpoint.
- 🌳 **Call & Type Hierarchies**: (Coming Soon) Query multi-hop code relationships computed by the language servers.
- 🔄 **Procedural Refactoring**: (Coming Soon) Perform symbol operations like `rename`, `extract`, `auto import` through the API.
- 🧩 **SDKs**: Libraries to get started calling `lsproxy` in popular languages.
    

## <a name="getting-started">Getting started</a>
Run one MCP server per project root. The server uses the current working directory as the workspace unless overridden.

```bash
cargo run --bin lsp-mcp -- --workspace-root /path/to/project
```

Configure your MCP client to launch the server with the project root as the working directory. The server communicates over stdio.

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
