; Go Reference Query
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

; Method calls via selector
(call_expression
  function: (selector_expression
    field: (field_identifier) @name)) @reference.function-call

; Package-qualified function calls
(call_expression
  function: (selector_expression
    operand: (identifier) @name)) @reference.package-reference

; Type references
(type_identifier) @name @reference.type-reference

; Struct instantiation (composite literals)
(composite_literal
  type: (type_identifier) @name) @reference.class-instantiation

; Qualified type instantiation
(composite_literal
  type: (qualified_type
    name: (type_identifier) @name)) @reference.class-instantiation

; Field access via selector
(selector_expression
  field: (field_identifier) @name) @reference.field-access

; Import path references
(import_spec
  name: (package_identifier) @name) @reference.import
