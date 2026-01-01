; Python Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Function definitions
(function_definition
  name: (identifier) @name) @definition.function

; Decorated function definitions
(decorated_definition
  definition: (function_definition
    name: (identifier) @name)) @definition.function

; Class definitions
(class_definition
  name: (identifier) @name) @definition.class

; Decorated class definitions
(decorated_definition
  definition: (class_definition
    name: (identifier) @name)) @definition.class

; Top-level variable assignments (module-level)
(expression_statement
  (assignment
    left: (identifier) @name)) @definition.variable

; Tuple unpacking at module level
(expression_statement
  (assignment
    left: (pattern_list
      (identifier) @name))) @definition.variable

; Local variable assignments (inside functions - will be filtered in Rust)
(assignment
  left: (identifier) @name) @definition.local-variable

; Annotated assignments
(expression_statement
  (assignment
    left: (identifier) @name
    type: (type))) @definition.variable

; Augmented assignments (+=, -=, etc.)
(expression_statement
  (augmented_assignment
    left: (identifier) @name)) @definition.variable
