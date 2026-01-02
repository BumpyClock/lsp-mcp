; TSX Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers
(identifier) @identifier

; Property identifiers (object properties, method names)
(property_identifier) @identifier

; Type identifiers (class names, interface names, type names)
(type_identifier) @identifier

; Shorthand property identifiers
(shorthand_property_identifier) @identifier

; This keyword
(this) @identifier

; JSX identifiers (component names in JSX elements)
(jsx_opening_element
  name: (identifier) @identifier)

(jsx_closing_element
  name: (identifier) @identifier)

(jsx_self_closing_element
  name: (identifier) @identifier)
