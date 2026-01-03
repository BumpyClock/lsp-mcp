Token Efficiency

  The LspClient query returned ~2,500 tokens for 45 references.

  Much of it is repetitive:
  src/lsp/languages/clang.rs (2 refs)
    [call] Line 14:45: `lsp::{DiagnosticsStore, JsonRpcHandler, LspClient, PendingRequests, ProcessHandler},`
    [call] Line 38:6: `impl LspClient for ClangdClient {`

  src/lsp/languages/csharp.rs (2 refs)
    [call] Line 3:45: `lsp::{DiagnosticsStore, JsonRpcHandler, LspClient, PendingRequests, ProcessHandler},`
    [call] Line 24:6: `impl LspClient for CSharpClient {`

  ... (same pattern 8 more times)

  I'm seeing the same import line repeated 10 times with minor variations. That's not efficient.

  Actionability

  Depends on what I'm trying to do:

  | Goal                           | Current Output  | What I Actually Need        |
  |--------------------------------|-----------------|-----------------------------|
  | "How widely used is this?"     | Overkill        | Count + file list           |
  | "What implements this trait?"  | Buried in noise | Just the impl X for Y lines |
  | "Will my change break things?" | Good            | File list is enough         |
  | "Understand a specific usage"  | Good            | Need the context            |

  What Would Be Better

  Compact summary by default:
  References to `LspClient` (45 total)
  Definition: src/lsp/client/trait_def.rs:30
  Re-export: src/lsp/client/mod.rs:10

  Implementations (10):
    ClangdClient, CSharpClient, GoplsClient, JdtlsClient, PhpactorClient,
    JediClient, RubyClient, RustAnalyzerClient, TypeScriptLanguageClient

  Used in: registry.rs (20), manager/core.rs (3), manager/startup.rs (2)

  Detail on demand:
  findReferences(symbol: "LspClient", detail: true)  // current verbose output
  findReferences(symbol: "LspClient", limit: 10)     // first 10 only

  Verdict

  Current output is:
  - Not token efficient - too verbose for common cases
  - Actionable but noisy - information is there, but I have to wade through it
  - One-size-fits-all - no way to get a summary vs full detail

  For most agent use cases, I need "what and where" not "show me every line".