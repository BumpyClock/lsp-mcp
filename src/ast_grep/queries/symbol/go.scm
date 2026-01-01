; Go Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Function declarations
(function_declaration
  name: (identifier) @name) @definition.function

; Method declarations
(method_declaration
  name: (field_identifier) @name) @definition.method

; Type declarations (structs, interfaces, type aliases)
(type_declaration
  (type_spec
    name: (type_identifier) @name)) @definition.type

; Struct types specifically
(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (struct_type))) @definition.struct

; Interface types specifically
(type_declaration
  (type_spec
    name: (type_identifier) @name
    type: (interface_type))) @definition.interface

; Variable declarations
(var_declaration
  (var_spec
    name: (identifier) @name)) @definition.variable

; Short variable declarations
(short_var_declaration
  left: (expression_list
    (identifier) @name)) @definition.variable

; Constant declarations
(const_declaration
  (const_spec
    name: (identifier) @name)) @definition.constant

; Package clause
(package_clause
  (package_identifier) @name) @definition.module
