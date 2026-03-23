" Vim syntax file for the Fiber programming language (.fib)
if exists("b:current_syntax")
  finish
endif

" Keywords
syntax keyword fiberKeyword fn var const if else for break continue return
syntax keyword fiberKeyword extern defer type struct enum union as switch when

" Builtin types
syntax keyword fiberType void bool char string never
syntax keyword fiberType int8 int16 int32 int64
syntax keyword fiberType uint8 uint16 uint32 uint64
syntax keyword fiberType float32 float64

" Boolean and null literals
syntax keyword fiberBoolean true false
syntax keyword fiberNull null

" Comments
syntax match fiberComment "//.*$"

" String literals
syntax region fiberString start=+"+ skip=+\\"+ end=+"+

" Character literals
syntax match fiberChar "'.'"
syntax match fiberChar "'\\[nrt\\''\"0]'"
syntax match fiberChar "'\\x[0-9a-fA-F][0-9a-fA-F]'"

" Number literals (decimal, hex, binary, octal, float)
syntax match fiberNumber "\<[0-9][0-9_]*\>"
syntax match fiberNumber "\<0x[0-9a-fA-F][0-9a-fA-F_]*\>"
syntax match fiberNumber "\<0b[01][01_]*\>"
syntax match fiberNumber "\<0o[0-7][0-7_]*\>"
syntax match fiberFloat  "\<[0-9][0-9_]*\.[0-9][0-9_]*\>"

" Operators
syntax match fiberOperator "[+\-*/%&|^~!<>=]\+"
syntax match fiberOperator "->"
syntax match fiberOperator "\.\*"
syntax match fiberOperator "\.\&"
syntax match fiberOperator "\.\["

" Highlight linkages
highlight default link fiberKeyword  Keyword
highlight default link fiberType     Type
highlight default link fiberBoolean  Boolean
highlight default link fiberNull     Constant
highlight default link fiberComment  Comment
highlight default link fiberString   String
highlight default link fiberChar     Character
highlight default link fiberNumber   Number
highlight default link fiberFloat    Float
highlight default link fiberOperator Operator

let b:current_syntax = "fiber"
