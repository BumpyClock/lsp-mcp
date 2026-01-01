; PHP Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Names (function names, class names, etc.)
(name) @identifier

; Variable names
(variable_name
  (name) @identifier)

; Qualified names (namespaced names)
(qualified_name
  (name) @identifier)

; Namespace names
(namespace_name
  (name) @identifier)

; Member access names
(member_access_expression
  name: (name) @identifier)

; Scoped call names
(scoped_call_expression
  name: (name) @identifier)

; Class constant access
(class_constant_access_expression
  (name) @identifier)
