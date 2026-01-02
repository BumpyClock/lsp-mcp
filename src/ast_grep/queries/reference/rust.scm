; Rust Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - identifiers
(identifier) @name @reference.all-references

; All references - type identifiers
(type_identifier) @name @reference.all-references

; All references - field identifiers
(field_identifier) @name @reference.all-references

; Function calls - direct calls
(call_expression
  function: (identifier) @name) @reference.function-call

; Method calls
(call_expression
  function: (field_expression
    field: (field_identifier) @name)) @reference.function-call

; Macro invocations
(macro_invocation
  macro: (identifier) @name) @reference.macro-invocation

; Scoped macro invocations (e.g., std::println!)
(macro_invocation
  macro: (scoped_identifier
    name: (identifier) @name)) @reference.macro-invocation

; Type references in type annotations
(type_identifier) @name @reference.type-reference

; Struct/enum instantiation
(struct_expression
  name: (type_identifier) @name) @reference.class-instantiation

; Scoped struct instantiation (e.g., module::Struct {})
(struct_expression
  name: (scoped_type_identifier
    name: (type_identifier) @name)) @reference.class-instantiation

; Use declarations (imports)
(use_declaration
  argument: (scoped_identifier
    name: (identifier) @name)) @reference.import

; Field access
(field_expression
  field: (field_identifier) @name) @reference.field-access

; Attribute references (e.g., #[derive(Debug)])
(attribute_item
  (attribute
    (identifier) @name)) @reference.decorator
