; Rust Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Functions
(function_item
  name: (identifier) @name) @definition.function

; Structs
(struct_item
  name: (type_identifier) @name) @definition.struct

; Enums
(enum_item
  name: (type_identifier) @name) @definition.enum

; Traits
(trait_item
  name: (type_identifier) @name) @definition.trait

; Type aliases
(type_item
  name: (type_identifier) @name) @definition.type

; Impl blocks (inherent implementation)
(impl_item
  type: (type_identifier) @name) @definition.implementation

; Trait implementations - capture the type being implemented for
(impl_item
  trait: (_)
  type: (type_identifier) @name) @definition.implementation

; Constants
(const_item
  name: (identifier) @name) @definition.constant

; Static items
(static_item
  name: (identifier) @name) @definition.global

; Module definitions
(mod_item
  name: (identifier) @name) @definition.module

; Methods inside impl blocks
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @name) @definition.method))

; Macro definitions
(macro_definition
  name: (identifier) @name) @definition.function
