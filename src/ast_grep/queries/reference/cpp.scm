; C++ Reference Query
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

; Method calls via field expression
(call_expression
  function: (field_expression
    field: (field_identifier) @name)) @reference.function-call

; Qualified function calls (namespace::function)
(call_expression
  function: (qualified_identifier
    name: (identifier) @name)) @reference.function-call

; Scoped function calls (Class::method)
(call_expression
  function: (qualified_identifier
    scope: (namespace_identifier) @name)) @reference.namespace-reference

; Object instantiation (new ClassName())
(new_expression
  type: (type_identifier) @name) @reference.class-instantiation

; Type references
(type_identifier) @name @reference.type-reference

; Template type references
(template_type
  name: (type_identifier) @name) @reference.type-reference

; Template arguments
(template_argument_list
  (type_descriptor
    type: (type_identifier) @name)) @reference.type-reference

; Field access
(field_expression
  field: (field_identifier) @name) @reference.field-access

; Namespace references
(namespace_identifier) @name @reference.namespace-reference

; Using declarations
(using_declaration
  (identifier) @name) @reference.import

; Include directives (header references)
(preproc_include
  path: (string_literal) @name) @reference.import

; Base class specifiers (inheritance)
(base_class_clause
  (type_identifier) @name) @reference.type-reference
