# LSP-MCP Tool Evaluation Prompt (UdaanUI, repeatable + 3 TS targets)

  You are an agentic coding LLM. Test the lsp-mcp server tools in this repo and give blunt, critical feedback on output quality. The goal is to optimize for token efficiency and decision-ready info so
  an agent avoids redundant tool calls 80-90% of the time.

  Start a background process to watch `.lsp-mcp/logs/{session-id}.log`. When you use the mcp tools debug logs will be written here. Verify that the lsp-mcp tools are giving you the expected output. If there's a discrepancy then investigate the logs to spot where it might be. Then give me a report

  The MCP uses **1-based** positions.

  ## Hard Constraints
  - No edits. No builds/tests. Read-only analysis.
  - Do **not** optimize for parallel tool calls. Instead, run tool calls **sequentially** and validate each result before moving on.
  - After **every** tool call, do a quick sanity-check that the response is correct and decision-usable:
    - Returned locations exist, are in the expected file(s), and match the expected symbol.
    - Ranges/positions are consistent with **1-based** indexing and the queried location.
    - Results are non-empty when they should be (and not wildly over/under-inclusive).
    - **CRITICAL** If anything looks wrong (empty, wrong file/symbol, wrong ranges, surprising truncation, etc.) or if a response is unexpected (something you would flag as a possible issue), then stop and investigate the logs to see if you can spot what might be causing it:
    - **CRITICAL** Inspect `.lsp-mcp/logs/lsp-mcp-debug.log` around the time of the call and try to identify what went wrong (e.g. file resolution, workspace root, LSP error, server capability mismatch, request/response shape). 
    - Include the suspected root cause + the most relevant log excerpt summary in the bug report, tied to the specific tool call/input.
  - Use **both relative and absolute paths** as specified below.
  - For any “absolute path” inputs in this prompt:
    - The prompt uses `$HOME/...` placeholders. **First resolve `$HOME` to the real absolute path** on the machine you’re running on (e.g. `echo "$HOME"`).
    - Then **expand `$HOME/...` into a concrete absolute path** and use that expanded path in actual tool inputs.
    - If the expanded path doesn’t exist locally, **find the actual absolute path** to the referenced file/project on disk and use that instead (while keeping the prompt’s relative-path targets unchanged).
  - If a tool fails or returns empty data, call it out and explain the impact.

  ## Required Output Format
  - **Markdown** with `##` / `###` headings, bullet lists, and comparison tables as needed.
  - Show only minimal snippets (no huge dumps).
  - Include file + line (and column when relevant). State indexing (1-based).

 
  ## Scenario A (TypeScript, multi-file change; no edits)
  Pretend you need to add a new `strategy` option to `scoreMember`, change internal scoring logic, and thread it through all call sites.

  **Decision-readiness questions:**
  - Where would you change the scoring logic, and what paths would be affected?
  - Which call sites must be updated? Provide file + line.
  - Which types/interfaces should the new option touch?
  - Any diagnostics already present that could confound changes?

  ## Scenario B (use Target Set B instead to test boundary/semantics changes on boolean predicates.
  - If Rust exists, pick a predicate in Rust and follow the original Rust scenario rules.

  **TS fallback task:**
  Pretend you need to clarify semantics for `isAttendanceBelowPromotion` (e.g., treat `"70%"` as **not** below threshold, and explicitly handle `"70.0%"`). 
  
  ## Scenario C (TypeScript, permission filtering; no edits)
  Pretend you need to add a `mode` option to `filterNavigationByPermissions` (e.g., `"strict"` vs `"lenient"`) that changes how `needsPermission` is interpreted, then thread it through call sites.

  **Decision-readiness questions:**
  - Where does the filtering logic live and what branches would change?
  - Which call sites require updates? Provide file + line.
  - Any existing diagnostics in the files involved?

  ## Scenario D (TypeScript, hook semantics; no edits)
  Pretend you need to add an option to `useFileViewerState` called `resetDelayMs` to control the timeout when the dialog closes. Thread it to call sites.

  **Decision-readiness questions:**
  - Where does the reset timer live and how would the option alter behavior?
  - Which call sites require updates? Provide file + line.
  - Any diagnostics in the hook or call-site files?

  ## Scenario E (TypeScript, ambiguous search-based discovery; no edits)
  Pretend you need to find all places in the codebase where user input is validated against a threshold value, and where the validation logic might need to handle edge cases differently (e.g., boundary conditions, null/undefined handling, type coercion). You don't know the exact function names, but you suspect there are multiple validation patterns scattered across different modules.

  **Decision-readiness questions:**
  - What are the different validation patterns used for threshold checks? Provide file + line for each pattern found.
  - Which of these validations might have edge case issues (e.g., strict vs loose equality, type mismatches, missing null checks)?
  - Are there any shared utilities or helpers for threshold validation that should be centralized?
  - What call sites would be affected if you standardized the validation approach?
  - Any existing diagnostics that highlight potential issues in these validation areas?

  ---

  ## Required Output Structure
  1) Short verdict summary (2-4 sentences max)
  2) Per-tool evaluation:
     - Tool
     - Scenario
     - Minimum info needed
     - lsp-mcp actionability (Yes / Partial / No)
     - Redundant-call savings (Yes / Partial / No)
     - Token efficiency (High / Medium / Low)
     - Key gaps
  3) lsp-mcp vs native LSP comparison table (if native available)
  4) Top improvement recommendations (ranked)

  **Do not include raw multi-page tool outputs.**