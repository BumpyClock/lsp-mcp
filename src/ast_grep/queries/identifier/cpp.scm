; C++ Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers
(identifier) @identifier

; Field identifiers (class/struct members)
(field_identifier) @identifier

; Type identifiers
(type_identifier) @identifier

; Namespace identifiers
(namespace_identifier) @identifier

; Qualified identifiers (namespace::name)
(qualified_identifier
  name: (identifier) @identifier)

; Template type names
(template_type
  name: (type_identifier) @identifier)

; This pointer
(this) @identifier
