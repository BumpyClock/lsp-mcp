; TSX Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - identifiers
(identifier) @name @reference.all-references

; All references - property identifiers
(property_identifier) @name @reference.all-references

; All references - type identifiers
(type_identifier) @name @reference.all-references

; Function calls - direct calls
(call_expression
  function: (identifier) @name) @reference.function-call

; Method calls
(call_expression
  function: (member_expression
    property: (property_identifier) @name)) @reference.function-call

; Decorators (TypeScript experimental decorators)
(decorator
  (identifier) @name) @reference.decorator

; Decorator with call expression
(decorator
  (call_expression
    function: (identifier) @name)) @reference.decorator

; JSX component renders - opening elements
(jsx_opening_element
  name: (identifier) @name) @reference.component-render

; JSX component renders - self-closing elements
(jsx_self_closing_element
  name: (identifier) @name) @reference.component-render

; JSX component renders - member expressions (e.g., <Module.Component />)
(jsx_opening_element
  name: (member_expression
    property: (property_identifier) @name)) @reference.component-render

(jsx_self_closing_element
  name: (member_expression
    property: (property_identifier) @name)) @reference.component-render

; Class instantiation (new ClassName())
(new_expression
  constructor: (identifier) @name) @reference.class-instantiation

; Type references
(type_identifier) @name @reference.type-reference
