; Java Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Class declarations
(class_declaration
  name: (identifier) @name) @definition.class

; Interface declarations
(interface_declaration
  name: (identifier) @name) @definition.interface

; Enum declarations
(enum_declaration
  name: (identifier) @name) @definition.enum

; Record declarations
(record_declaration
  name: (identifier) @name) @definition.class

; Annotation type declarations
(annotation_type_declaration
  name: (identifier) @name) @definition.interface

; Method declarations
(method_declaration
  name: (identifier) @name) @definition.method

; Constructor declarations
(constructor_declaration
  name: (identifier) @name) @definition.method

; Field declarations
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @name)) @definition.field

; Local variable declarations
(local_variable_declaration
  declarator: (variable_declarator
    name: (identifier) @name)) @definition.local-variable

; Package declarations
(package_declaration
  (scoped_identifier) @name) @definition.module
