; TypeScript Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Function declarations
(function_declaration
  name: (identifier) @name) @definition.function

; Function signatures (in interfaces/type declarations)
(function_signature
  name: (identifier) @name) @definition.function

; Class declarations
(class_declaration
  name: (type_identifier) @name) @definition.class

; Abstract class declarations
(abstract_class_declaration
  name: (type_identifier) @name) @definition.class

; Interface declarations
(interface_declaration
  name: (type_identifier) @name) @definition.interface

; Type alias declarations
(type_alias_declaration
  name: (type_identifier) @name) @definition.type

; Enum declarations
(enum_declaration
  name: (identifier) @name) @definition.enum

; Method definitions
(method_definition
  name: (property_identifier) @name) @definition.method

; Method signatures
(method_signature
  name: (property_identifier) @name) @definition.method

; Abstract method signatures
(abstract_method_signature
  name: (property_identifier) @name) @definition.method

; Module/namespace declarations
(module
  name: (identifier) @name) @definition.module

(internal_module
  name: (identifier) @name) @definition.module

; Variable declarations (const/let/var at module level)
(lexical_declaration
  (variable_declarator
    name: (identifier) @name)) @definition.variable

; Arrow functions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @definition.function

; Variable declarations (var)
(variable_declaration
  (variable_declarator
    name: (identifier) @name)) @definition.variable

; Arrow functions with var
(variable_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @definition.function

; Class properties
(public_field_definition
  name: (property_identifier) @name) @definition.property

; Property signatures in interfaces
(property_signature
  name: (property_identifier) @name) @definition.field
