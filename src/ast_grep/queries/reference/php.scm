; PHP Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - names
(name) @name @reference.all-references

; Function calls - direct calls
(function_call_expression
  function: (name) @name) @reference.function-call

; Function calls - qualified names
(function_call_expression
  function: (qualified_name
    (name) @name)) @reference.function-call

; Method calls
(member_call_expression
  name: (name) @name) @reference.function-call

; Static method calls
(scoped_call_expression
  name: (name) @name) @reference.function-call

; Class instantiation (new ClassName())
(object_creation_expression
  (name) @name) @reference.class-instantiation

; Class instantiation with qualified name
(object_creation_expression
  (qualified_name
    (name) @name)) @reference.class-instantiation

; Attribute usage (PHP 8.0+ #[Attribute])
(attribute
  (name) @name) @reference.attribute-usage

; Attribute usage with qualified name
(attribute
  (qualified_name
    (name) @name)) @reference.attribute-usage

; Type references in function parameters
(simple_parameter
  type: (named_type
    (name) @name)) @reference.type-reference

; Type references in property declarations
(property_declaration
  type: (named_type
    (name) @name)) @reference.type-reference

; Type references in return types
(function_definition
  return_type: (named_type
    (name) @name)) @reference.type-reference

; Class constant access (captures the name in the access expression)
(class_constant_access_expression
  (name) @name) @reference.class-reference

; Static property access
(scoped_property_access_expression
  (name) @name) @reference.class-reference

; Trait use statements
(use_declaration
  (name) @name) @reference.trait-usage
