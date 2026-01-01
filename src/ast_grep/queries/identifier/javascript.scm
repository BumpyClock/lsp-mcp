; JavaScript Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers
(identifier) @identifier

; Property identifiers (object properties, method names)
(property_identifier) @identifier

; Shorthand property identifiers
(shorthand_property_identifier) @identifier

; This keyword
(this) @identifier
