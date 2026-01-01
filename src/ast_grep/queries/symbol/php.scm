; PHP Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Class declarations
(class_declaration
  name: (name) @name) @definition.class

; Abstract class declarations
(class_declaration
  (abstract_modifier)
  name: (name) @name) @definition.class

; Final class declarations
(class_declaration
  (final_modifier)
  name: (name) @name) @definition.class

; Interface declarations
(interface_declaration
  name: (name) @name) @definition.interface

; Trait declarations
(trait_declaration
  name: (name) @name) @definition.trait

; Enum declarations (PHP 8.1+)
(enum_declaration
  name: (name) @name) @definition.enum

; Function definitions
(function_definition
  name: (name) @name) @definition.function

; Method declarations
(method_declaration
  name: (name) @name) @definition.method

; Static method declarations
(method_declaration
  (static_modifier)
  name: (name) @name) @definition.method

; Abstract method declarations
(method_declaration
  (abstract_modifier)
  name: (name) @name) @definition.method

; Property declarations
(property_declaration
  (property_element
    (variable_name
      (name) @name))) @definition.property

; Static property declarations
(property_declaration
  (static_modifier)
  (property_element
    (variable_name
      (name) @name))) @definition.property

; Const declarations (class constants)
(const_declaration
  (const_element
    (name) @name)) @definition.constant

; Namespace definitions
(namespace_definition
  name: (namespace_name) @name) @definition.module

; Constructor property promotion (PHP 8.0+)
(property_promotion_parameter
  name: (variable_name
    (name) @name)) @definition.property

; Global variable definitions
(global_declaration
  (variable_name
    (name) @name)) @definition.global
