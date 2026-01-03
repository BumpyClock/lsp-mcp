# LSP-MCP Tool Evaluation Prompt (UdaanUI, repeatable + 3 TS targets)

  You are an agentic coding LLM. Test the lsp-mcp server tools in this repo and give blunt, critical feedback on output quality. The goal is to optimize for token efficiency and decision-ready info so
  an agent avoids redundant tool calls 80-90% of the time.

  Start a background process to watch `.lsp-mcp/logs/lsp-mcp-debug.log`. When you use the mcp tools debug logs will be written here. Verify that the lsp-mcp tools are giving you the expected output. If there's a discrepancy then investigate the logs to spot where it might be. Then give me a report

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

  ---

  ## Target Set A (Search Scoring, multi-file)
  **Core function (implementation):**
  - `scoreMember` at `src/utilities/memberScoring.ts:36:17`

  **Overload signatures (definition resolution behavior):**
  - `src/utilities/memberScoring.ts:28:17`
  - `src/utilities/memberScoring.ts:35:17`

  **Type/interface targets:**
  - `LibraryMember` in `src/utilities/memberScoring.ts:6:18`
  - `LibraryMember` in `src/Components/LibrarySearchCombobox/types.ts:7:18`

  **Call sites (multiple refs):**
  - `scoreMember` at `src/Components/LibrarySearchCombobox/LibrarySearchCombobox.tsx:83:23`
  - `scoreMember` at `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Pages/Library/Checkout/TypeAheadSearch.tsx:74:20`
  - `scoreMember` at `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Pages/Library/Checkout/TypeAheadSearch.tsx:75:20`

  **Related helper (coverage):**
  - `normalizeMember` call at `src/Components/LibrarySearchCombobox/LibrarySearchCombobox.tsx:78:36`
  - `normalizeMember` definition at `src/Components/LibrarySearchCombobox/utils.ts:12:17`

  ---

  ## Target Set B (Promotion Criteria, boolean predicates)
  **Predicate functions:**
  - `isCumulativeGradeBelowB` at `src/utilities/promotionCriteriaUtils.ts:11:14`
  - `isAttendanceBelowPromotion` at `src/utilities/promotionCriteriaUtils.ts:30:14`

  **Call sites:**
  - `isCumulativeGradeBelowB` at `src/Components/Cards/StudentCard/StudentCard.tsx:320:42`
  - `isCumulativeGradeBelowB` at `src/Components/Cards/StudentCard/tabs/TopicTab.tsx:45:30`
  - `isCumulativeGradeBelowB` at `src/Pages/Class/ClassTeacherPage/StudentRoster/components/StudentRosterTable.tsx:243:44`
  - `isAttendanceBelowPromotion` at `src/Components/Cards/StudentCard/StudentCard.tsx:495:40`
  - `isAttendanceBelowPromotion` at `src/Components/Cards/StudentCard/tabs/AttendanceTab.tsx:41:30`
  - `isAttendanceBelowPromotion` at `src/Pages/Class/ClassTeacherPage/StudentRoster/components/StudentRosterTable.tsx:206:40`

  ---

  ## Target Set C (Navigation permission filtering, multi-file)
  **Core function:**
  - `filterNavigationByPermissions` at `src/Components/Navigation/navigationConstants.ts:185:14`

  **Call sites:**
  - `filterNavigationByPermissions` at `src/Components/Navigation/TopNav.tsx:31:25`
  - `filterNavigationByPermissions` at `src/Components/Navigation/MobileExpandableHeader.tsx:80:12`

  ---

  ## Target Set D (Hook + reducer, hooks-heavy)
  **Core hook + reducer:**
  - `useFileViewerState` at `src/Pages/Class/ClassTeacherPage/Grading/hooks/useFileViewerState.ts:126:14`
  - `fileViewerReducer` at `src/Pages/Class/ClassTeacherPage/Grading/hooks/useFileViewerState.ts:41:7`

  **Call site:**
  - `useFileViewerState` at `src/Pages/Class/ClassTeacherPage/Grading/HomeworkFileViewer.tsx:119:30`

  ---

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
  Pretend you need to clarify semantics for `isAttendanceBelowPromotion` (e.g., treat `"70%"` as **not** below threshold, and explicitly handle `"70.0%"`). Identify tests or missing tests.

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

  ---

  ## Required Tool Calls (exact inputs)

  ### 1) `goToDefinition`
  - Relative: `scoreMember` at `src/Components/LibrarySearchCombobox/LibrarySearchCombobox.tsx:83:23`
  - Absolute: `scoreMember` at `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Pages/Library/Checkout/TypeAheadSearch.tsx:74:20`
  - Relative: `normalizeMember` at `src/Components/LibrarySearchCombobox/LibrarySearchCombobox.tsx:78:36`
  - Relative: `filterNavigationByPermissions` at `src/Components/Navigation/TopNav.tsx:31:25`
  - Absolute: `filterNavigationByPermissions` at `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Components/Navigation/MobileExpandableHeader.tsx:80:12`
  - Relative: `useFileViewerState` at `src/Pages/Class/ClassTeacherPage/Grading/HomeworkFileViewer.tsx:119:30`

  ### 2) `findReferences`
  - Relative: `findReferences(symbol: "scoreMember", path: "src/utilities/memberScoring.ts")`
  - Absolute: `findReferences(symbol: "isAttendanceBelowPromotion", path: "$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/utilities/promotionCriteriaUtils.ts")`
  - Relative: `findReferences(symbol: "filterNavigationByPermissions", path: "src/Components/Navigation/navigationConstants.ts")`
  - Relative: `findReferences(symbol: "useFileViewerState", path: "src/Pages/Class/ClassTeacherPage/Grading/hooks/useFileViewerState.ts")`

  ### 3) `callHierarchy` (outgoing)
  - Relative: `scoreMember` at `src/utilities/memberScoring.ts:36:17`
  - Absolute: `isAttendanceBelowPromotion` at `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/utilities/promotionCriteriaUtils.ts:30:14`
  - Relative: `filterNavigationByPermissions` at `src/Components/Navigation/navigationConstants.ts:185:14`
  - Relative: `useFileViewerState` at `src/Pages/Class/ClassTeacherPage/Grading/hooks/useFileViewerState.ts:126:14`

  ### 4) `hover` (include_definition = true)
  - Relative: `LibraryMember` at `src/Components/LibrarySearchCombobox/LibrarySearchCombobox.tsx:75:18`
  - Absolute: `scoreMember` at `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Pages/Library/Checkout/TypeAheadSearch.tsx:74:20`
  - Relative: `isAttendanceBelowPromotion` at `src/Pages/Class/ClassTeacherPage/StudentRoster/components/StudentRosterTable.tsx:206:40`
  - Relative: `filterNavigationByPermissions` at `src/Components/Navigation/TopNav.tsx:31:25`
  - Relative: `useFileViewerState` at `src/Pages/Class/ClassTeacherPage/Grading/HomeworkFileViewer.tsx:119:30`

  ### 5) `documentSymbol`
  - Relative: `src/utilities/memberScoring.ts`
  - Absolute: `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Components/LibrarySearchCombobox/utils.ts`
  - Relative: `src/utilities/promotionCriteriaUtils.ts`
  - Relative: `src/Components/Navigation/navigationConstants.ts`
  - Relative: `src/Pages/Class/ClassTeacherPage/Grading/hooks/useFileViewerState.ts`

  ### 6) `workspaceSymbol`
  Run queries exactly:
  - `scoreMember`
  - `LibraryMember`
  - `normalizeMember`
  - `isAttendanceBelowPromotion`
  - `isCumulativeGradeBelowB`
  - `LibrarySearchCombobox`
  - `filterNavigationByPermissions`
  - `useFileViewerState`
  - `fileViewerReducer`

  ### 7) `getDiagnostics`
  - Relative: `src/utilities/memberScoring.ts`
  - Absolute: `$HOME/Projects/Gurukul/SchoolWeb/UdaanUI/src/Components/LibrarySearchCombobox/LibrarySearchCombobox.tsx`
  - Relative: `src/utilities/promotionCriteriaUtils.ts`
  - Relative: `src/Components/Navigation/navigationConstants.ts`
  - Relative: `src/Pages/Class/ClassTeacherPage/Grading/hooks/useFileViewerState.ts`
  - Relative: `src/Pages/Class/ClassTeacherPage/Grading/HomeworkFileViewer.tsx`

  ### 8) `findReferencedSymbols`
  If available, run on:
  - `scoreMember` at `src/utilities/memberScoring.ts:36:17`
  If not available, explicitly note missing tool and impact.

  ## Comparison vs Native LSP
  If a native LSP tool exists, run equivalent queries and compare; otherwise, skip and say so.

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