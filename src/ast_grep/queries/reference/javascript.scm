; JavaScript Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - identifiers
(identifier) @name @reference.all-references

; All references - property identifiers
(property_identifier) @name @reference.all-references

; Function calls - direct calls
(call_expression
  function: (identifier) @name) @reference.function-call

; Method calls
(call_expression
  function: (member_expression
    property: (property_identifier) @name)) @reference.function-call

; Class instantiation (new ClassName())
(new_expression
  constructor: (identifier) @name) @reference.class-instantiation

; Property access
(member_expression
  property: (property_identifier) @name) @reference.property-access

; Import specifiers
(import_specifier
  name: (identifier) @name) @reference.import

; Namespace imports
(namespace_import
  (identifier) @name) @reference.import

; Shorthand property access in destructuring
(shorthand_property_identifier) @name @reference.all-references
