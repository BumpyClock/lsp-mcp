; Java Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers
(identifier) @identifier

; Type identifiers (class names, interface names)
(type_identifier) @identifier

; Scoped identifiers (package names, qualified names)
(scoped_identifier
  name: (identifier) @identifier)

; This keyword
(this) @identifier

; Super keyword
(super) @identifier
