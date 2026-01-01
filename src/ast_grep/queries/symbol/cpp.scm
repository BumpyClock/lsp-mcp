; C++ Symbol Query
; Captures symbol definitions with @name for identifier and @definition.<rule_id> for full node

; Class definitions
(class_specifier
  name: (type_identifier) @name) @definition.class

; Struct definitions
(struct_specifier
  name: (type_identifier) @name) @definition.struct

; Union definitions
(union_specifier
  name: (type_identifier) @name) @definition.struct

; Enum definitions
(enum_specifier
  name: (type_identifier) @name) @definition.enum

; Function declarations (prototypes)
(declaration
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

; Function definitions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

; Method definitions (class member functions)
(function_definition
  declarator: (function_declarator
    declarator: (field_identifier) @name)) @definition.method

; Qualified method definitions (ClassName::method)
(function_definition
  declarator: (function_declarator
    declarator: (qualified_identifier
      name: (identifier) @name))) @definition.method

; Typedef declarations
(type_definition
  declarator: (type_identifier) @name) @definition.type

; Namespace definitions
(namespace_definition
  name: (namespace_identifier) @name) @definition.module

; Variable declarations
(declaration
  declarator: (init_declarator
    declarator: (identifier) @name)) @definition.variable

; Simple variable declarations
(declaration
  declarator: (identifier) @name) @definition.variable

; Template class declarations
(template_declaration
  (class_specifier
    name: (type_identifier) @name)) @definition.class

; Template function declarations
(template_declaration
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @name))) @definition.function

; Macro definitions
(preproc_function_def
  name: (identifier) @name) @definition.function

; Object-like macro definitions
(preproc_def
  name: (identifier) @name) @definition.constant
