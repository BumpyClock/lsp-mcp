; Ruby Identifier Query
; Captures all identifier occurrences in a file with @identifier

; Regular identifiers (local variables, method names, etc.)
(identifier) @identifier

; Constants (class names, module names)
(constant) @identifier

; Instance variables
(instance_variable) @identifier

; Class variables
(class_variable) @identifier

; Global variables
(global_variable) @identifier

; Simple symbols
(simple_symbol) @identifier

; Hash key symbols
(hash_key_symbol) @identifier

; Self keyword
(self) @identifier

; Scope resolution names
(scope_resolution
  name: (constant) @identifier)
