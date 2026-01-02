; Python Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers (variables, functions, classes, etc.)
(identifier) @identifier

; Attribute access (object.attribute)
(attribute
  attribute: (identifier) @identifier)
