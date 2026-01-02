; C# Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - identifiers
(identifier) @name @reference.all-references

; Function/method calls - invocation expressions
(invocation_expression
  function: (identifier) @name) @reference.function-call

; Method calls on objects
(invocation_expression
  function: (member_access_expression
    name: (identifier) @name)) @reference.function-call

; Static method calls
(invocation_expression
  function: (member_access_expression
    expression: (identifier)
    name: (identifier) @name)) @reference.function-call

; Class instantiation (new ClassName())
(object_creation_expression
  type: (identifier) @name) @reference.class-instantiation

; Class instantiation with generic types
(object_creation_expression
  type: (generic_name
    (identifier) @name)) @reference.class-instantiation

; Attribute usage ([Attribute])
(attribute
  name: (identifier) @name) @reference.attribute-usage

; Attribute usage with qualified name
(attribute
  name: (qualified_name
    (identifier) @name)) @reference.attribute-usage

; Type references in variable declarations
(variable_declaration
  type: (identifier) @name) @reference.type-reference

; Type references in method parameters
(parameter
  type: (identifier) @name) @reference.type-reference

; Base class/interface references
(base_list
  (identifier) @name) @reference.type-reference
