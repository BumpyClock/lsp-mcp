; Go Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers
(identifier) @identifier

; Field identifiers (struct fields)
(field_identifier) @identifier

; Type identifiers
(type_identifier) @identifier

; Package identifiers
(package_identifier) @identifier

; Blank identifier (_)
(blank_identifier) @identifier
