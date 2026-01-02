; JavaScript Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node
; NOTE: JavaScript uses TSX grammar (tree-sitter-typescript LANGUAGE_TSX), so queries must match TSX node types

; Function declarations
(function_declaration
  name: (identifier) @name) @definition.function

; Generator function declarations
(generator_function_declaration
  name: (identifier) @name) @definition.function

; Class declarations (uses type_identifier in TSX grammar)
(class_declaration
  name: (type_identifier) @name) @definition.class

; Method definitions
(method_definition
  name: (property_identifier) @name) @definition.method

; Variable declarations (const/let)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name)) @definition.variable

; Arrow functions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @definition.function

; Function expressions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (function_expression))) @definition.function

; Variable declarations (var)
(variable_declaration
  (variable_declarator
    name: (identifier) @name)) @definition.variable

; Arrow functions with var
(variable_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @definition.function

; Function expressions with var
(variable_declaration
  (variable_declarator
    name: (identifier) @name
    value: (function_expression))) @definition.function

; Class properties/fields
(public_field_definition
  name: (property_identifier) @name) @definition.property
