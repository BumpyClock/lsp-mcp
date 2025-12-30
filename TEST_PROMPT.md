# LSP-MCP Tool Evaluation Prompt

You are an agentic coding LLM. Your job is to test the lsp-mcp server tools in this repo and give blunt, critical feedback on the quality of their outputs. The goal is to optimize for token efficiency and decision-ready information so that an AI coding agent can avoid redundant tool calls 80-90 percent of the time. Do not be nice. Do not gaslight. Tell the unvarnished truth. Compare the lsp-mcp tool outputs to native LSP tool outputs where possible, if you don't have a native LSP tool skip the comparison.

the MCP uses 1-based position. 

## Core Goal

Evaluate whether each tool response is actionable enough for real agentic coding work, not edge cases. Judge output quality based on: how quickly it enables a correct coding decision, how much extra exploration it avoids, and whether it is concise without omitting essential details.

## Instructions

- Use parallel tool calls whenever queries are independent.
- Try with both absolute and relative file paths.
- Use 2-3 representative code locations across the repo (not edge cases). Include at least:
  - A function or method
  - A type or interface
  - A call site with multiple references
- Run each lsp-mcp tool on those locations and compare with the native LSP tool (if available) using the same queries.
- Keep outputs compact; include only the minimal excerpts needed to support your evaluation.
- If a tool output is missing crucial info, say exactly what is missing and how that blocks decision-making.

## Formatting Requirements (Markdown)

Your final response must be **nicely formatted Markdown** that is easy to skim.

- **Headings:** Use `##` and `###` headings to mirror the “Required Output Structure” sections.
- **Bullets:** Prefer short bullet lists. Avoid long paragraphs.
- **Tables:** Use Markdown tables for comparisons. Keep cells short and decision-oriented.
- **Tool outputs:** Do NOT paste raw multi-page tool outputs.
  - Quote only the *minimum* snippet needed, using fenced code blocks:
    - Use ```text``` unless the snippet is clearly a language block (e.g. ```rust```, ```ts```).
  - If a tool returns structured data (JSON), show only the fields you relied on (truncate aggressively).
- **Citations:** When referencing a location, always include at least `file` + `line` (and `character` when relevant), and specify whether indexing is 0-based or 1-based if there’s any ambiguity.
- **Consistency:** Use the exact labels `Yes / Partial / No` and `High / Medium / Low` where requested, so the report is easy to compare across runs.

## Scenario-Based Evaluation (Required)

Do at least ONE end-to-end **dry-run** of a “real coding agent” task (not just poking symbols). The goal is to force tool usage at actual decision points: understanding unfamiliar code, scoping a multi-file change, and sanity-checking expected fallout.

**Important constraint:** You are **NOT** actually implementing the change. Do **not** edit files, do not run builds/tests, and do not claim outcomes that require execution. You are only evaluating whether the tool outputs would be sufficient for a competent agent to proceed with high confidence.

### Scenario A (TypeScript/JavaScript, multi-file change): Add an option + update call sites (no edits)

**Context:** You’ve been asked to make an algorithm/service class more realistic and configurable.

**Pick targets (so this works in any repo):**
- Pick a file that defines a **class or exported function** that is used in multiple places (2+ call sites).
- Pick a **type/interface** related to it (or create a hypothetical one you would add) that would need to change to thread a new option through.
- Pick at least one call site that lives in a different file/module.

**Hypothetical task (do NOT implement):**
- Change some internal “cost/score/timeout/retry” logic in the core class/function (something that requires following helper calls, not a one-liner).
- Introduce a new option (e.g. `mode`, `strategy`, `heuristic`, `timeoutMs`) with a default.
- Thread the option through all relevant call sites.

**Decision-readiness criteria (what you must be able to answer after tool calls):**
- Where exactly would you change cost accumulation, and what existing code paths would be affected?
- What *specific* call sites would need updates (list file paths + approximate locations), and how confident are you that you found all of them?
- What types/interfaces should the new `Heuristic` option touch, and where should it live?
- What diagnostics (if any) already exist in the relevant files that could confound your change?

**Tool pressure points to test (use these tools intentionally, read-only):**
- Use `find_references` on the core class/function to find and update all instantiations/call sites.
- Use `find_definition` to jump from a representative call site to the definition and then to the internal logic you would change.
- Use `call_hierarchy` on the “entry point” method/function (outgoing) to quickly map helper methods you’ll need to touch.
- Use `hover` on the new option types to verify inferred types and defaults.
- Use `get_diagnostics` (without edits) to see whether existing diagnostics would complicate the hypothetical change.

### Scenario B (Rust, correctness refactor): Tighten a boundary/semantics rule (no edits)

**Context:** There is a boundary/semantics rule that can be ambiguous (inclusive vs exclusive, 0-based vs 1-based, normalization rules, etc.). You’ve been asked to make it explicit and consistent.

**Pick targets (so this works in any repo):**
- Pick a function that implements a **non-trivial boolean predicate** (e.g. `contains`, `matches`, `is_valid`, `normalize_*`) and has tests or multiple call sites.
- Ensure at least one downstream call site exists (even if only in tests).

**Hypothetical task (do NOT implement):**
- Update the predicate semantics to match a clearly stated convention (write down the convention in your evaluation).
- Identify which tests would change (or which ones are missing).
- Identify downstream callers impacted (if any) and explain the risk.

**Decision-readiness criteria:**
- What is the current behavior (inclusive/exclusive/etc.) based on code + tests, and what would you change it to?
- What test cases would need to change (which ones, and how)?
- What downstream callers might rely on the current semantics, and how risky is the change based on references/call hierarchy?
- What diagnostics (if any) exist in the affected Rust files?

### Scenario execution rules

- Treat this like a real PR **up to (but not including) the edit**: start from a symptom/request, locate code, scope the changes, and identify risks—without modifying anything.
- While doing the scenario, keep a running tally per tool: did it let you decide immediately, or did you need follow-up calls to locate missing context?
- After finishing the scenario, do a quick “spot-check” run of the remaining tools on 1-2 additional symbols so every tool is exercised at least once.

## Per-Tool Evaluation Criteria

For each tool, include:

1) Scenario you would use the tool for
2) The minimum information needed to proceed with a coding decision
3) Whether the lsp-mcp response provides that minimum in 80-90 percent of real cases
4) Whether the response prevents redundant follow-up calls most of the time
5) Token efficiency assessment (High, Medium, Low)
6) Any noise, ambiguity, or missing context that slows you down

## Required Output Structure

1) Short verdict summary (2-4 sentences max)
2) Per-tool evaluation  with these details:
   - Tool
   - Scenario
   - Minimum info needed
   - lsp-mcp actionability (Yes / Partial / No)
   - Redundant-call savings (Yes / Partial / No)
   - Token efficiency (High / Medium / Low)s
   - Key gaps
3) lsp-mcp vs native LSP comparison table with these columns (only when you have a native LSP tool):
   - Tool
   - Better for agentic coding (lsp-mcp / native / tie)
   - Why (1-2 sentences)
4) Top improvement recommendations (ranked, highest impact first)

## Tools to Exercise

Run and evaluate these lsp-mcp tools:

- call_hierarchy
- definitions_in_file
- find_definition
- find_referenced_symbols
- find_references
- get_diagnostics
- hover
- workspace_symbol

If a tool fails or returns empty data, note that explicitly and explain how it affects usability.
