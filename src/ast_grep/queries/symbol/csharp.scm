; C# Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Namespace declarations (simple names)
(namespace_declaration
  name: (identifier) @name) @definition.module

; Namespace declarations (qualified names)
(namespace_declaration
  name: (qualified_name) @name) @definition.module

; File-scoped namespace declarations
(file_scoped_namespace_declaration
  name: (identifier) @name) @definition.module

(file_scoped_namespace_declaration
  name: (qualified_name) @name) @definition.module

; Class declarations
(class_declaration
  name: (identifier) @name) @definition.class

; Interface declarations
(interface_declaration
  name: (identifier) @name) @definition.interface

; Struct declarations
(struct_declaration
  name: (identifier) @name) @definition.struct

; Enum declarations
(enum_declaration
  name: (identifier) @name) @definition.enum

; Record declarations
(record_declaration
  name: (identifier) @name) @definition.class

; Method declarations
(method_declaration
  name: (identifier) @name) @definition.method

; Property declarations
(property_declaration
  name: (identifier) @name) @definition.property

; Field declarations
(field_declaration
  (variable_declaration
    (variable_declarator
      (identifier) @name))) @definition.field

; Local variable declarations
(local_declaration_statement
  (variable_declaration
    (variable_declarator
      (identifier) @name))) @definition.local-variable

; Event declarations
(event_declaration
  name: (identifier) @name) @definition.property

; Delegate declarations
(delegate_declaration
  name: (identifier) @name) @definition.type

; Constructor declarations
(constructor_declaration
  name: (identifier) @name) @definition.method

; Constant declarations
(field_declaration
  (modifier) @_const
  (variable_declaration
    (variable_declarator
      (identifier) @name))
  (#eq? @_const "const")) @definition.constant
