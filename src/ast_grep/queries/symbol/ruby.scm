; Ruby Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Method definitions
(method
  name: (identifier) @name) @definition.method

; Singleton methods (class methods)
(singleton_method
  name: (identifier) @name) @definition.method

; Class definitions
(class
  name: (constant) @name) @definition.class

; Class definitions with scope resolution
(class
  name: (scope_resolution
    name: (constant) @name)) @definition.class

; Singleton class definitions
(singleton_class
  value: (constant) @name) @definition.class

; Module definitions
(module
  name: (constant) @name) @definition.module

; Module definitions with scope resolution
(module
  name: (scope_resolution
    name: (constant) @name)) @definition.module

; Constant assignments
(assignment
  left: (constant) @name) @definition.constant

; Global variables
(global_variable) @name @definition.global

; Instance variables
(instance_variable) @name @definition.field

; Class variables
(class_variable) @name @definition.field
