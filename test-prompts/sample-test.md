# lsp-mcp tool evaluation prompt (C, C#, Python, Rust)

You are evaluating the **lsp-mcp** server’s MCP tools on the code under `sample_project/` for these languages:
- C
- C#
- Python
- Rust

**Hard constraint:** do not make *any* edits to the repository (no file changes, no formatting, no commits).

## Tools to test (coverage baseline)
Test and report on **each** of the following MCP tools:
- `mcp__lsp-mcp__health`
- `mcp__lsp-mcp__hover`
- `mcp__lsp-mcp__goToDefinition`
- `mcp__lsp-mcp__goToImplementation`
- `mcp__lsp-mcp__callHierarchy`
- `mcp__lsp-mcp__workspaceSymbol`
- `mcp__lsp-mcp__documentSymbol`
- `mcp__lsp-mcp__findReferencedSymbols`
- `mcp__lsp-mcp__findReferences`
- `mcp__lsp-mcp__findIdentifier`
- `mcp__lsp-mcp__semanticSearch`
- `mcp__lsp-mcp__getDiagnostics`

If any tool cannot be exercised for a given language, you must still include it in the report and mark it as **Not Tested** with a precise reason (unsupported by server, missing LSP, no results possible, error, etc.).

## What to produce
For each language, assess:
1. **Token efficiency**: how much context and output is needed to complete tasks.
2. **Quality**: correctness and completeness (right file/range/symbol, missing results, duplicates, stale/incorrect locations).
3. **Actionability**: how directly the output lets a developer take the next step (jump to location, pick correct symbol, resolve ambiguity, etc.).

## Required method (per language)
Run **parallel sub-agents**, one per language (C, C#, Python, Rust). Each sub-agent must:

### 1) Minimal familiarization (read a few files first)
Read a small number of representative files (target 2–5) in `sample_project/<language>/` to learn real symbol names.
- List the files read and why they were selected.
- Extract a short list of target symbols to test (functions/types/methods/variables).

### 2) Tool-by-tool exercise
For each tool in the baseline list:
- Perform at least **1–3** targeted queries using the previously identified symbols.
- Prefer queries that include both “easy” and “ambiguous” cases (e.g., overloaded names, similarly named symbols, cross-file references).

### 3) Verification against ground truth
Verify each tool result against the source:
- Open only the smallest necessary snippet(s) to confirm symbol identity and location.
- Record verification as **Pass / Partial / Fail**.
  - **Pass**: correct location/range + correct target
  - **Partial**: mostly correct but missing results / incomplete metadata / ambiguous output
  - **Fail**: wrong file/range/target or unusable output

### 4) Token efficiency accounting (lightweight, consistent)
For each tool invocation:
- Provide a rough token estimate for **request** and **response** (consistency > precision).
- Mark verbosity: **too terse / right-sized / too verbose**.
- Note whether a smaller response would remain actionable, or whether missing details forced extra tool calls/file reads.

## Output format (strict)
Provide a single consolidated report with the following sections.

### A) Executive summary (table)
One row per language. Columns:
- Tool coverage (tested/total)
- Token efficiency score (1–5)
- Quality score (1–5)
- Actionability score (1–5)
- Top 2 wins
- Top 2 gaps

### B) Per-language deep dive (repeat for C, C#, Python, Rust)
For each language, include:
1. **Files read** (list)
2. **Symbols chosen** (list)
3. **Tool-by-tool results** (one subsection per tool)
   - Query/intent (what you tried to do)
   - Returned summary (don’t paste huge blobs; summarize)
   - Verification (Pass/Partial/Fail + why)
   - Token notes (request/response estimates + verbosity)
   - Actionability notes (what a dev can do next; what’s missing)
4. **Issues & suggested improvements** (prioritized)

### C) Cross-language comparison
- Where behavior is consistent vs language-specific
- Most impactful improvements to make outputs more actionable and token-efficient

## Failure handling
If something fails (timeouts, missing LSP, server errors, empty results):
- Capture the exact error (verbatim)
- Provide minimal repro steps
- State whether it appears language-specific or systemic

Begin by calling `mcp__lsp-mcp__health`, then launch four parallel sub-agents (C, C#, Python, Rust).