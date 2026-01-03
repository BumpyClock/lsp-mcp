# lsp-mcp tool evaluation prompt (C, C#, Python, Rust)

spawn parallel background agent tasks to evaluate lsp-mcp tools on sample_project for each language in the following set:
- C
- C#
- Python
- Rust
- TypeScript

## Required method (per language)
Run **parallel sub-agents**, one per language (C, C#, Python, Rust). Each sub-agent must:


spawn each agent with a distinct identity (e.g., "lsp-mcp-eval-C", "lsp-mcp-eval-CSharp", etc.) with the following command:
`cd sample_project/{language} && cz -p "$(cat ../../test-prompts/subagent-testprompt.md)"`

give each agent the promt in `subagent-testprompt.md` 

## Output format
### Per-language deep dive (repeat for C, C#, Python, Rust)
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
