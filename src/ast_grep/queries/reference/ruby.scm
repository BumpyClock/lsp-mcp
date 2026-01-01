; Ruby Reference Query
; Captures reference patterns with @name for identifier and @reference.<rule_id> for context

; All references - identifiers
(identifier) @name @reference.all-references

; All references - constants (class/module names)
(constant) @name @reference.all-references

; Method calls - simple
(call
  method: (identifier) @name) @reference.function-call

; Method calls with receiver
(call
  receiver: (identifier) @name) @reference.object-reference

; Method calls on constants (Class.method)
(call
  receiver: (constant) @name) @reference.class-reference

; Class instantiation (ClassName.new)
(call
  receiver: (constant) @name
  method: (identifier) @method_name
  (#eq? @method_name "new")) @reference.class-instantiation

; Scope resolution (Module::Class)
(scope_resolution
  name: (constant) @name) @reference.type-reference

; Scope resolution scope
(scope_resolution
  scope: (constant) @name) @reference.namespace-reference

; Superclass reference
(superclass
  (constant) @name) @reference.type-reference

; Module inclusion
(call
  method: (identifier) @method_name
  arguments: (argument_list
    (constant) @name)
  (#match? @method_name "^(include|extend|prepend)$")) @reference.mixin

; Constant references (type-like usage)
(constant) @name @reference.type-reference

; Symbol references
(simple_symbol) @name @reference.symbol

; Instance variable access
(instance_variable) @name @reference.instance-variable

; Class variable access
(class_variable) @name @reference.class-variable
