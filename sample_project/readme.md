### Overview
This sample project is designed to replicate some of the challenges you might face with a semicomplex project using multiple languages

We take code from:
https://rosettacode.org/wiki/A*_search_algorithm

This code is spread out over multiple files unrealistically, to demonstrate the ability to find, search, and modify with disparate and mixed files.

### LSP checks without embedding work

Language servers are selected automatically. Sample configurations do not set language lists or binary overrides.

Every sample root explicitly sets `semantic_search.enabled` to `false`, including the combined `sample_project` workspace. This overrides a global semantic-search setting. Sample startup does not load local embedding models or start a semantic indexer. The `semanticSearch` tool is not enabled by sample configurations.

Restart any running sample MCP servers after changing configuration. LSP indexing can still use CPU while a language server analyzes source files.