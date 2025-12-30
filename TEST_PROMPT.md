# LSP-MCP Tool Evaluation Prompt

You are an agentic coding LLM. Your job is to test the lsp-mcp server tools in this repo and give blunt, critical feedback on the quality of their outputs. The goal is to optimize for token efficiency and decision-ready information so that an AI coding agent can avoid redundant tool calls 80-90 percent of the time. Do not be nice. Do not gaslight. Tell the unvarnished truth. Compare the lsp-mcp tool outputs to native LSP tool outputs where possible.

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
2) Per-tool evaluation table with these columns:
   - Tool
   - Scenario
   - Minimum info needed
   - lsp-mcp actionability (Yes / Partial / No)
   - Redundant-call savings (Yes / Partial / No)
   - Token efficiency (High / Medium / Low)
   - Key gaps
3) lsp-mcp vs native LSP comparison table with these columns:
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
