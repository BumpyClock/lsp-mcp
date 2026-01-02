// ABOUTME: Hover markdown parser using pulldown-cmark.
// ABOUTME: Parses LSP hover contents into structured blocks for signature extraction.

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

/// A parsed code block from hover markdown
#[derive(Debug, Clone)]
pub(crate) struct CodeBlock {
    /// Programming language hint (e.g., "typescript", "rust")
    #[allow(dead_code)] // Preserved for future language-specific handling
    pub language: Option<String>,
    /// The code content (without fence markers)
    pub content: String,
    /// True if this block appears after an @example tag
    pub is_after_example_tag: bool,
}

/// Result of parsing hover markdown
#[derive(Debug)]
pub(crate) struct ParsedHover {
    pub code_blocks: Vec<CodeBlock>,
    pub text_content: String,
}

/// Parses hover markdown text into structured blocks using pulldown-cmark.
pub(crate) fn parse_hover_markdown(text: &str) -> ParsedHover {
    let parser = Parser::new(text);

    let mut code_blocks = Vec::new();
    let mut text_parts = Vec::new();
    let mut current_code = String::new();
    let mut current_lang: Option<String> = None;
    let mut in_code_block = false;
    let mut seen_example_tag = false;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                current_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let l = lang.to_string();
                        if l.is_empty() {
                            None
                        } else {
                            Some(l)
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                if !current_code.trim().is_empty() {
                    code_blocks.push(CodeBlock {
                        language: current_lang.take(),
                        content: current_code.trim().to_string(),
                        is_after_example_tag: seen_example_tag,
                    });
                }
                current_code.clear();
                in_code_block = false;
            }
            Event::Text(text) | Event::Code(text) => {
                if in_code_block {
                    current_code.push_str(&text);
                } else {
                    // Check if this text contains an example tag
                    if is_example_tag(&text) {
                        seen_example_tag = true;
                    } else {
                        text_parts.push(text.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    ParsedHover {
        code_blocks,
        text_content: text_parts.join(" ").trim().to_string(),
    }
}

/// Detects example tags regardless of markdown formatting.
/// Handles: @example, *@example*, **@example**, `@example`, Example:, etc.
fn is_example_tag(text: &str) -> bool {
    let normalized = text
        .trim()
        .trim_matches(|c: char| c == '*' || c == '_' || c == '`')
        .trim_end_matches(':')
        .trim()
        .to_ascii_lowercase();

    normalized == "@example" || normalized == "example" || normalized.starts_with("@example ")
}

/// Checks if code block starts with a definition keyword.
/// Returns false for variable assignments (const x = value).
fn is_definition_block(content: &str) -> bool {
    let first_line = content.lines().next().unwrap_or("").trim();

    // Variable assignments are not definitions
    if is_variable_assignment(content) {
        return false;
    }

    let definition_keywords = [
        // TypeScript/JavaScript
        "interface ",
        "type ",
        "class ",
        "function ",
        "enum ",
        "export interface ",
        "export type ",
        "export class ",
        "export function ",
        "export enum ",
        "export default ",
        "declare ",
        "abstract class ",
        // Rust
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "pub(super) fn ",
        "async fn ",
        "pub async fn ",
        "struct ",
        "pub struct ",
        "enum ",
        "pub enum ",
        "trait ",
        "pub trait ",
        "impl ",
        "impl<",
        "mod ",
        "pub mod ",
        "const ",
        "pub const ",
        "static ",
        "pub static ",
        "type ",
        "pub type ",
        // Python
        "def ",
        "async def ",
        "class ",
        // Go
        "func ",
        "type ",
        // C/C++
        "void ",
        "int ",
        "char ",
        "auto ",
        "template",
        "virtual ",
        "inline ",
    ];

    definition_keywords
        .iter()
        .any(|kw| first_line.starts_with(kw))
}

/// Checks if code block is a variable assignment (usage example).
fn is_variable_assignment(content: &str) -> bool {
    let trimmed = content.trim();
    let is_var = trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ");
    is_var && trimmed.contains('=')
}

/// Checks if a code block looks like an actual signature.
/// This is a fallback for blocks that don't start with definition keywords.
fn is_likely_signature(block: &str) -> bool {
    let trimmed = block.trim();

    // Empty blocks are not signatures
    if trimmed.is_empty() {
        return false;
    }

    // Variable assignments are usage examples, not signatures
    if is_variable_assignment(trimmed) {
        return false;
    }

    // Reference/pointer types starting with &, *, or [ are valid signatures
    if trimmed.starts_with('&') || trimmed.starts_with('*') || trimmed.starts_with('[') {
        return true;
    }

    // Single word without spaces/parens/colons is likely just a module name
    if !trimmed.contains(' ')
        && !trimmed.contains('(')
        && !trimmed.contains('<')
        && !trimmed.contains(':')
        && !trimmed.contains("->")
    {
        return false;
    }

    // Look for signature patterns by content
    trimmed.contains(": ")
        || trimmed.contains("->")
        || trimmed.contains("where ")
        || trimmed.contains('(')
        || (trimmed.contains('<') && trimmed.contains('>'))
        || trimmed.contains('[')
}

/// Selects signature from parsed hover using deterministic rules:
/// 1. First definition-like block BEFORE any example tag
/// 2. First valid signature block BEFORE any example tag
/// 3. Fall back to first definition-like block after example tag
/// 4. Fall back to any non-assignment code block
pub(crate) fn select_signature(parsed: &ParsedHover) -> Option<String> {
    // Priority 1: Definition blocks before example tag
    for block in &parsed.code_blocks {
        if !block.is_after_example_tag
            && is_definition_block(&block.content)
            && !is_variable_assignment(&block.content)
        {
            return Some(block.content.clone());
        }
    }

    // Priority 2: Any valid signature block before example tag
    for block in &parsed.code_blocks {
        if !block.is_after_example_tag
            && !is_variable_assignment(&block.content)
            && is_likely_signature(&block.content)
        {
            return Some(block.content.clone());
        }
    }

    // Priority 3: Definition blocks after example tag (rare fallback)
    for block in &parsed.code_blocks {
        if is_definition_block(&block.content) && !is_variable_assignment(&block.content) {
            return Some(block.content.clone());
        }
    }

    // Priority 4: Any valid signature block (last resort)
    for block in &parsed.code_blocks {
        if !is_variable_assignment(&block.content) && is_likely_signature(&block.content) {
            return Some(block.content.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_code_block() {
        let md = "```typescript\ninterface Foo\n```";
        let parsed = parse_hover_markdown(md);
        assert_eq!(parsed.code_blocks.len(), 1);
        assert_eq!(
            parsed.code_blocks[0].language,
            Some("typescript".to_string())
        );
        assert!(parsed.code_blocks[0].content.contains("interface Foo"));
        assert!(!parsed.code_blocks[0].is_after_example_tag);
    }

    #[test]
    fn test_parse_code_block_without_language() {
        let md = "```\nfn foo() -> i32\n```";
        let parsed = parse_hover_markdown(md);
        assert_eq!(parsed.code_blocks.len(), 1);
        assert_eq!(parsed.code_blocks[0].language, None);
        assert!(parsed.code_blocks[0].content.contains("fn foo"));
    }

    #[test]
    fn test_example_tag_marks_subsequent_blocks() {
        let md = "```ts\ninterface Foo\n```\n\n@example\n\n```ts\nconst x = 1\n```";
        let parsed = parse_hover_markdown(md);
        assert_eq!(parsed.code_blocks.len(), 2);
        assert!(!parsed.code_blocks[0].is_after_example_tag);
        assert!(parsed.code_blocks[1].is_after_example_tag);
    }

    #[test]
    fn test_is_example_tag_variants() {
        assert!(is_example_tag("@example"));
        assert!(is_example_tag("@Example"));
        assert!(is_example_tag("@EXAMPLE"));
        assert!(is_example_tag("Example:"));
        assert!(is_example_tag("example"));
        assert!(is_example_tag("@example usage"));

        assert!(!is_example_tag("Some text"));
        assert!(!is_example_tag("For example"));
        assert!(!is_example_tag(""));
    }

    #[test]
    fn test_is_definition_block() {
        // TypeScript/JavaScript
        assert!(is_definition_block("interface Foo"));
        assert!(is_definition_block("type Bar = string"));
        assert!(is_definition_block("class MyClass"));
        assert!(is_definition_block("function helper()"));
        assert!(is_definition_block("export interface Foo"));
        assert!(is_definition_block("export type Bar"));

        // Rust
        assert!(is_definition_block("fn main()"));
        assert!(is_definition_block("pub fn helper()"));
        assert!(is_definition_block("pub(crate) fn internal()"));
        assert!(is_definition_block("struct Point"));
        assert!(is_definition_block("pub struct Point"));
        assert!(is_definition_block("impl Point"));
        assert!(is_definition_block("impl<T> From<T>"));

        // Not definitions
        assert!(!is_definition_block("const x = 5"));
        assert!(!is_definition_block("let y = 10"));
        assert!(!is_definition_block("some_module"));
    }

    #[test]
    fn test_is_variable_assignment() {
        assert!(is_variable_assignment("const x = 5"));
        assert!(is_variable_assignment("let y = 10"));
        assert!(is_variable_assignment("var z = 'hello'"));
        assert!(is_variable_assignment("const config: Config = {}"));

        assert!(!is_variable_assignment("const FOO: number"));
        assert!(!is_variable_assignment("interface Foo"));
        assert!(!is_variable_assignment("fn main()"));
    }

    #[test]
    fn test_is_likely_signature() {
        assert!(is_likely_signature("fn foo() -> i32"));
        assert!(is_likely_signature("x: string"));
        assert!(is_likely_signature("Result<T, E>"));
        assert!(is_likely_signature("&str"));
        assert!(is_likely_signature("*const u8"));

        assert!(!is_likely_signature("some_module"));
        assert!(!is_likely_signature("const x = 5"));
        assert!(!is_likely_signature(""));
    }

    #[test]
    fn test_select_prefers_definition_before_example() {
        let md = r#"```typescript
interface NavItem
```

Description.

@example

```typescript
const item: NavItem = {};
```"#;
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed).unwrap();
        assert!(
            sig.contains("interface NavItem"),
            "Expected interface, got: {}",
            sig
        );
        assert!(
            !sig.contains("const item"),
            "Should not contain example code"
        );
    }

    #[test]
    fn test_select_with_italic_example_tag() {
        let md = r#"```typescript
interface NavItem
```

Navigation item interface.

*@example*

```typescript
const navItem: NavItem = {
  id: 'dashboard',
  label: 'Dashboard',
};
```"#;
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed).unwrap();
        assert!(
            sig.contains("interface NavItem"),
            "Expected interface, got: {}",
            sig
        );
    }

    #[test]
    fn test_select_falls_back_to_example_definition() {
        // Only example code, but it's a definition
        let md = "@example\n\n```ts\nfunction helper(): void\n```";
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed);
        assert!(sig.is_some());
        assert!(sig.unwrap().contains("function helper"));
    }

    #[test]
    fn test_select_skips_module_name_block() {
        let md = r#"```rust
lsproxy
```

```rust
pub async fn initialize_manager(path: &Path) -> Result<Manager>
```

---

Initialize a workspace manager."#;
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed).unwrap();
        assert!(
            sig.contains("pub async fn initialize_manager"),
            "Expected function signature, got: {}",
            sig
        );
        assert!(
            !sig.contains("lsproxy") || sig.contains("pub async fn"),
            "Should skip module name"
        );
    }

    #[test]
    fn test_select_returns_none_for_empty_content() {
        let md = "";
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed);
        assert!(sig.is_none());
    }

    #[test]
    fn test_select_returns_none_for_only_text() {
        let md = "This is just documentation without any code.";
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed);
        assert!(sig.is_none());
    }

    #[test]
    fn test_text_content_extraction() {
        let md = r#"```typescript
interface Foo
```

This is the description.

More details here."#;
        let parsed = parse_hover_markdown(md);
        assert!(parsed.text_content.contains("This is the description"));
        assert!(parsed.text_content.contains("More details here"));
    }

    #[test]
    fn test_multiple_example_tags() {
        let md = r#"```typescript
interface Foo
```

@example First example
```typescript
const a = 1;
```

@example Second example
```typescript
const b = 2;
```"#;
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed).unwrap();
        assert!(
            sig.contains("interface Foo"),
            "Expected interface, got: {}",
            sig
        );
    }

    #[test]
    fn test_real_tsserver_navitem_payload() {
        let md = r#"```typescript
export interface NavItem {
  id?: string;
}
```

Navigation item interface.

@example
```typescript
// RECOMMENDED: Provide stable IDs for reliable persistence
const navItem: NavItem = {
  id: "dashboard-overview",
  label: "Dashboard",
  path: "/dashboard",
};
```"#;
        let parsed = parse_hover_markdown(md);
        let sig = select_signature(&parsed).unwrap();
        assert!(
            sig.contains("interface NavItem"),
            "Expected interface definition, got: {}",
            sig
        );
        assert!(
            !sig.contains("const navItem"),
            "Should not return example"
        );
    }

}
