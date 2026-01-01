; Rust Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers (variables, functions, etc.)
(identifier) @identifier

; Field identifiers (struct fields, method names)
(field_identifier) @identifier

; Type identifiers (struct names, enum names, trait names)
(type_identifier) @identifier

; Self keyword
(self) @identifier

; Super keyword
(super) @identifier

; Crate keyword
(crate) @identifier
