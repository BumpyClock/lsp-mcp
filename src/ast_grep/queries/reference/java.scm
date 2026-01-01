; Java Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - identifiers
(identifier) @name @reference.all-references

; All references - type identifiers
(type_identifier) @name @reference.all-references

; Method invocations
(method_invocation
  name: (identifier) @name) @reference.function-call

; Object method invocations
(method_invocation
  object: (identifier) @name) @reference.object-reference

; Object creation (new ClassName())
(object_creation_expression
  type: (type_identifier) @name) @reference.class-instantiation

; Type references in declarations
(type_identifier) @name @reference.type-reference

; Generic type arguments
(type_arguments
  (type_identifier) @name) @reference.type-reference

; Field access
(field_access
  field: (identifier) @name) @reference.field-access

; Annotations (decorators)
(marker_annotation
  name: (identifier) @name) @reference.decorator

; Annotations with arguments
(annotation
  name: (identifier) @name) @reference.decorator

; Scoped annotations
(annotation
  name: (scoped_identifier
    name: (identifier) @name)) @reference.decorator

; Import declarations
(import_declaration
  (scoped_identifier
    name: (identifier) @name)) @reference.import

; Interface implementations
(super_interfaces
  (type_list
    (type_identifier) @name)) @reference.type-reference

; Class extensions
(superclass
  (type_identifier) @name) @reference.type-reference
