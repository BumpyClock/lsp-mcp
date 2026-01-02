; TypeScript Reference Query
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

; Class instantiation (new ClassName())
(new_expression
  constructor: (identifier) @name) @reference.class-instantiation

; Type references
(type_identifier) @name @reference.type-reference

; Generic type arguments
(type_arguments
  (type_identifier) @name) @reference.type-reference

; Property access
(member_expression
  property: (property_identifier) @name) @reference.property-access

; Import specifiers
(import_specifier
  name: (identifier) @name) @reference.import

; Namespace imports
(namespace_import
  (identifier) @name) @reference.import
