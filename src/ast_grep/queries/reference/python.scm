; Python Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references (generic identifier usage - filtered in Rust to exclude definitions)
(identifier) @name @reference.all-references

; Function calls - direct calls
(call
  function: (identifier) @name) @reference.function-call

; Method calls - attribute access calls
(call
  function: (attribute
    attribute: (identifier) @name)) @reference.function-call

; Decorators
(decorator
  (identifier) @name) @reference.decorator

; Decorated with call expression
(decorator
  (call
    function: (identifier) @name)) @reference.decorator

; Attribute decorators (e.g., @module.decorator)
(decorator
  (attribute
    attribute: (identifier) @name)) @reference.decorator

; Class instantiation (new objects)
(call
  function: (identifier) @name) @reference.class-instantiation
