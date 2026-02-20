; ModuleID = 'fib_module'
source_filename = "fib_module"

define i64 @main() {
entry:
  %x_alloca = alloca i64, align 8
  store i64 3, i64* %x_alloca, align 4
  %load_x = load i64, i64* %x_alloca, align 4
  ret i64 %load_x
}
