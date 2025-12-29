# LSP-MCP Tools Review for LLM Coding Agents

**Date:** 2024-12-29
**Reviewer:** Claude (Opus 4.5)
**Objective:** Evaluate which LSP-MCP tools provide value to LLM coding agents and identify noise/improvement opportunities.

---

## Executive Summary

The LSP-MCP server exposes 15 tools. After systematic testing, **only 6-7 tools provide unique value** that can't be replicated with standard file/grep tools. The rest either duplicate existing capabilities or are too specialized for general coding tasks.

**Recommendation:** Reduce the default tool set from 15 to 6-7 tools to minimize decision paralysis and context bloat for LLMs.

---

## Tool Evaluation

### TIER 1 - HIGH VALUE (Keep as core tools)

| Tool | Value | Noise Level | Analysis |
|------|-------|-------------|----------|
| **`definitions_in_file`** | ⭐⭐⭐⭐⭐ | Low | Excellent for understanding file structure. Returns clean JSON with symbol name, kind, position, and range. This is gold for LLMs—provides instant AST-like overview without parsing code. |
| **`hover`** | ⭐⭐⭐⭐⭐ | Low | Returns type info + docstrings. Critical for understanding what symbols actually *are*. Output is concise and actionable. Includes documentation when available. |
| **`find_definition`** | ⭐⭐⭐⭐ | Low | Core navigation capability. The `include_source_code` option is smart—lets LLM get definition + context in one call. Error handling for external crates is informative. |
| **`workspace_symbol`** | ⭐⭐⭐⭐ | Low | Essential for "find me the X class" queries. Clean output with kind and location. Fuzzy matching works well. |
| **`get_diagnostics`** | ⭐⭐⭐⭐ | Low | Critical for knowing if code is broken. File grouping + severity + source is exactly what an LLM needs. Can query single file or whole workspace. |

### TIER 2 - MEDIUM VALUE (Situational but useful)

| Tool | Value | Noise Level | Analysis |
|------|-------|-------------|----------|
| **`find_references`** | ⭐⭐⭐ | Medium-High | Useful but **output gets noisy fast**. 6 references = 6 context blocks with overlapping code. The `include_code_context_lines` option exacerbates this. |
| **`outgoing_calls`** | ⭐⭐⭐ | Low | Good for understanding what a function depends on. Output is clean with function signatures. |

### TIER 3 - LOW VALUE FOR LLMs (Consider removing or making opt-in)

| Tool | Value | Noise Level | Analysis |
|------|-------|-------------|----------|
| **`incoming_calls`** | ⭐⭐ | Medium | Rarely needed. An LLM can use `find_references` instead and infer callers. Overlaps significantly with references functionality. |
| **`prepare_call_hierarchy`** | ⭐ | Low | This is an *intermediate* LSP call—needs follow-up with incoming/outgoing. Not useful standalone for LLMs. |
| **`go_to_implementation`** | ⭐⭐ | Low | Only useful for interfaces/traits. Most coding tasks don't need this. Could be replaced with `find_references` + filtering. |
| **`find_referenced_symbols`** | ⭐⭐ | Medium | Interesting concept but **only works for Python/TS/C#**. The "what symbols does this function use" question is better answered by reading the code. |
| **`find_identifier`** | ⭐ | High | A "find text in file with optional position" tool. This is just grep with extra steps. LLMs already have Grep/Read tools. |
| **`list_files`** | ⭐ | High | Duplicates `ls` / `Glob` capabilities. LLMs don't need another way to list files. |
| **`read_source_code`** | ⭐ | High | Duplicates the `Read` tool. LLMs already have superior file reading tools with line numbers. |
| **`health`** | ⭐ | Low | Only useful for debugging the LSP server itself, not for coding tasks. |

---

## Detailed Test Results

### `definitions_in_file` - EXCELLENT

**Test:** `definitions_in_file("lsproxy/src/mcp.rs")`

**Output sample:**
```json
{
  "name": "LspMcpServer",
  "kind": "struct",
  "identifier_position": { "path": "lsproxy/src/mcp.rs", "position": { "line": 14, "character": 11 } },
  "file_range": { "path": "lsproxy/src/mcp.rs", "range": { "start": { "line": 14, "character": 0 }, "end": { "line": 16, "character": 1 } } }
}
```

**Verdict:** Clean, structured, exactly what an LLM needs to understand a file's structure.

---

### `hover` - EXCELLENT

**Test:** `hover("lsproxy/src/mcp.rs", line=14, character=11)`

**Output:**
```json
{
  "contents": "lsproxy::mcp\n\npub struct LspMcpServer {\n    service: LspService,\n}\n\nLSP MCP Server that exposes code navigation tools for a workspace.",
  "range": { "start": { "line": 14, "character": 11 }, "end": { "line": 14, "character": 23 } }
}
```

**Verdict:** Type info + documentation in one call. Essential for semantic understanding.

---

### `find_references` - NOISY

**Test:** `find_references("lsproxy/src/mcp.rs", line=14, character=11, include_code_context_lines=2)`

**Output:** 6 references, each with 5-line context blocks. Many blocks overlap, creating redundant output.

**Verdict:** Useful capability but output format needs work. See recommendations below.

---

### `find_definition` with external crate - GOOD ERROR HANDLING

**Test:** Navigating to `ToolHandler` trait (external dependency)

**Output:**
```
Error: File '/home/.../.cargo/registry/src/.../mcpkit-server-0.5.0/src/handler.rs' not found in workspace
```

**Verdict:** Clear, actionable error message. Could be improved by returning the external path so LLM can inform user.

---

### `find_referenced_symbols` - LIMITED

**Test:** Called on Rust file

**Output:**
```
Error: Not implemented: Find referenced symbols is only implemented for Python, TypeScript/JavaScript, and C#
```

**Verdict:** Language-specific limitation. Should be documented clearly or hidden for unsupported languages.

---

## Recommendations

### 1. Reduce Default Tool Count (15 → 6-7)

**Proposed "standard" preset:**
```json
["definitions_in_file", "find_definition", "find_references", "hover", "workspace_symbol", "get_diagnostics", "outgoing_calls"]
```

### 2. Output Noise Improvements

**`find_references` fixes:**
- Add `max_results: Option<u32>` parameter (default 10)
- Make `include_code_context_lines` default to `0` (just return positions)
- Add `compact: bool` flag that returns just `path:line` strings

**`definitions_in_file` enhancements:**
- Add `symbols_only: bool` that returns just `["LspMcpServer", "new", "definitions_in_file", ...]`
- Add `kind_filter: Option<Vec<String>>` to get only functions, structs, etc.

### 3. Deprecate Duplicate Tools

These tools duplicate Claude Code's built-in capabilities:
- `list_files` → Use `Glob`/`LS`
- `read_source_code` → Use `Read`
- `find_identifier` → Use `Grep`

Remove from default presets or mark as deprecated.

### 4. Merge Call Hierarchy Tools

Consolidate `prepare_call_hierarchy` + `incoming_calls` + `outgoing_calls` into:
```
call_graph(path, line, character, direction: "incoming" | "outgoing" | "both")
```

### 5. Proposed Preset Configuration

```json
{
  "minimal": ["definitions_in_file", "hover", "get_diagnostics"],
  "standard": ["definitions_in_file", "find_definition", "find_references", "hover", "workspace_symbol", "get_diagnostics"],
  "full": ["definitions_in_file", "find_definition", "find_references", "hover", "workspace_symbol", "get_diagnostics", "outgoing_calls", "incoming_calls", "go_to_implementation", "find_referenced_symbols"]
}
```

---

## What Works Well

1. **Summary lines before JSON** - The `"Found 24 symbols"` prefixes are exactly what LLMs need
2. **Error messages are specific** - `"No identifier found at position with 3 nearby matches"` is actionable
3. **`include_source_code` option** - Smart design that reduces round trips
4. **Tool filtering via config** - The preset system is well-designed

---

## GitHub Issues Created

1. **feat: reduce output noise in find_references and definitions_in_file** - Issue for Recommendation #2
2. **refactor: deprecate tools that duplicate Claude Code built-in capabilities** - Issue for Recommendation #3
3. **refactor: consolidate call hierarchy tools into single unified tool** - Issue for Recommendation #4
