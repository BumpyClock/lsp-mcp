; C# Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers
(identifier) @identifier

; Qualified names
(qualified_name
  (identifier) @identifier)

; Member access
(member_access_expression
  name: (identifier) @identifier)

; Generic names
(generic_name
  (identifier) @identifier)
