; ModuleID = 'src/main.fib'
source_filename = "src/main.fib"

@str = private unnamed_addr constant [9 x i8] c"Before: \00", align 1
@str.1 = private unnamed_addr constant [4 x i8] c"%d \00", align 1
@str.2 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@str.3 = private unnamed_addr constant [9 x i8] c"After:  \00", align 1
@str.4 = private unnamed_addr constant [4 x i8] c"%d \00", align 1
@str.5 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@str.6 = private unnamed_addr constant [12 x i8] c"Passes: %d\0A\00", align 1
@str.7 = private unnamed_addr constant [12 x i8] c"Swaps:  %d\0A\00", align 1
@str.8 = private unnamed_addr constant [9 x i8] c"Min: %d\0A\00", align 1
@str.9 = private unnamed_addr constant [9 x i8] c"Max: %d\0A\00", align 1

declare i32 @printf(ptr, ...)

declare ptr @malloc(i64)

declare void @free(ptr)

declare ptr @memcpy(ptr, ptr, i64)

declare void @exit(i32)

declare i64 @strlen(ptr)

declare i32 @puts(ptr)

define i32 @min_of(i32 %0, i32 %1) {
entry:
  %a_addr = alloca i32, align 4
  store i32 %0, ptr %a_addr, align 4
  %b_addr = alloca i32, align 4
  store i32 %1, ptr %b_addr, align 4
  %load_a = load i32, ptr %a_addr, align 4
  %load_b = load i32, ptr %b_addr, align 4
  %lttmp = icmp slt i32 %load_a, %load_b
  br i1 %lttmp, label %then, label %ifcont

ifcont:                                           ; preds = %entry
  %load_b2 = load i32, ptr %b_addr, align 4
  ret i32 %load_b2

then:                                             ; preds = %entry
  %load_a1 = load i32, ptr %a_addr, align 4
  ret i32 %load_a1
}

define i32 @max_of(i32 %0, i32 %1) {
entry:
  %a_addr = alloca i32, align 4
  store i32 %0, ptr %a_addr, align 4
  %b_addr = alloca i32, align 4
  store i32 %1, ptr %b_addr, align 4
  %load_a = load i32, ptr %a_addr, align 4
  %load_b = load i32, ptr %b_addr, align 4
  %gttmp = icmp sgt i32 %load_a, %load_b
  br i1 %gttmp, label %then, label %ifcont

ifcont:                                           ; preds = %entry
  %load_b2 = load i32, ptr %b_addr, align 4
  ret i32 %load_b2

then:                                             ; preds = %entry
  %load_a1 = load i32, ptr %a_addr, align 4
  ret i32 %load_a1
}

define i32 @main() {
entry:
  %data_addr = alloca [8 x i32], align 4
  %arrtmp = alloca [8 x i32], align 4
  %arr_elem_ptr = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 0
  store i32 64, ptr %arr_elem_ptr, align 4
  %arr_elem_ptr1 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 1
  store i32 34, ptr %arr_elem_ptr1, align 4
  %arr_elem_ptr2 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 2
  store i32 25, ptr %arr_elem_ptr2, align 4
  %arr_elem_ptr3 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 3
  store i32 12, ptr %arr_elem_ptr3, align 4
  %arr_elem_ptr4 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 4
  store i32 22, ptr %arr_elem_ptr4, align 4
  %arr_elem_ptr5 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 5
  store i32 11, ptr %arr_elem_ptr5, align 4
  %arr_elem_ptr6 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 6
  store i32 90, ptr %arr_elem_ptr6, align 4
  %arr_elem_ptr7 = getelementptr [8 x i32], ptr %arrtmp, i32 0, i32 7
  store i32 1, ptr %arr_elem_ptr7, align 4
  %arrload = load [8 x i32], ptr %arrtmp, align 4
  store [8 x i32] %arrload, ptr %data_addr, align 4
  %n_addr = alloca i32, align 4
  store i32 8, ptr %n_addr, align 4
  %swaps_addr = alloca i32, align 4
  store i32 0, ptr %swaps_addr, align 4
  %passes_addr = alloca i32, align 4
  store i32 0, ptr %passes_addr, align 4
  %calltmp = call i32 (ptr, ...) @printf(ptr @str)
  %i_addr = alloca i32, align 4
  store i32 0, ptr %i_addr, align 4
  br label %forcond

forcond:                                          ; preds = %forpost, %entry
  %load_i = load i32, ptr %i_addr, align 4
  %load_n = load i32, ptr %n_addr, align 4
  %lttmp = icmp slt i32 %load_i, %load_n
  br i1 %lttmp, label %forbody, label %afterloop

forbody:                                          ; preds = %forcond
  %load_i8 = load i32, ptr %i_addr, align 4
  %load_data = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp = alloca [8 x i32], align 4
  store [8 x i32] %load_data, ptr %arridxtmp, align 4
  %arr_idx_ptr = getelementptr [8 x i32], ptr %arridxtmp, i32 0, i32 %load_i8
  %arr_idx_load = load i32, ptr %arr_idx_ptr, align 4
  %calltmp9 = call i32 (ptr, ...) @printf(ptr @str.1, i32 %arr_idx_load)
  br label %forpost

forpost:                                          ; preds = %forbody
  %load_i10 = load i32, ptr %i_addr, align 4
  %addtmp = add i32 %load_i10, 1
  store i32 %addtmp, ptr %i_addr, align 4
  br label %forcond

afterloop:                                        ; preds = %forcond
  %calltmp11 = call i32 (ptr, ...) @printf(ptr @str.2)
  %i_addr12 = alloca i32, align 4
  store i32 0, ptr %i_addr12, align 4
  br label %forcond13

forcond13:                                        ; preds = %forpost15, %afterloop
  %load_i17 = load i32, ptr %i_addr12, align 4
  %load_n18 = load i32, ptr %n_addr, align 4
  %subtmp = sub i32 %load_n18, 1
  %lttmp19 = icmp slt i32 %load_i17, %subtmp
  br i1 %lttmp19, label %forbody14, label %afterloop16

forbody14:                                        ; preds = %forcond13
  %load_passes = load i32, ptr %passes_addr, align 4
  %addtmp20 = add i32 %load_passes, 1
  store i32 %addtmp20, ptr %passes_addr, align 4
  %j_addr = alloca i32, align 4
  store i32 0, ptr %j_addr, align 4
  br label %forcond21

forpost15:                                        ; preds = %afterloop24
  %load_i59 = load i32, ptr %i_addr12, align 4
  %addtmp60 = add i32 %load_i59, 1
  store i32 %addtmp60, ptr %i_addr12, align 4
  br label %forcond13

afterloop16:                                      ; preds = %forcond13
  %calltmp61 = call i32 (ptr, ...) @printf(ptr @str.3)
  %i_addr62 = alloca i32, align 4
  store i32 0, ptr %i_addr62, align 4
  br label %forcond63

forcond21:                                        ; preds = %forpost23, %forbody14
  %load_j = load i32, ptr %j_addr, align 4
  %load_n25 = load i32, ptr %n_addr, align 4
  %load_i26 = load i32, ptr %i_addr12, align 4
  %subtmp27 = sub i32 %load_n25, %load_i26
  %subtmp28 = sub i32 %subtmp27, 1
  %lttmp29 = icmp slt i32 %load_j, %subtmp28
  br i1 %lttmp29, label %forbody22, label %afterloop24

forbody22:                                        ; preds = %forcond21
  %load_j30 = load i32, ptr %j_addr, align 4
  %load_data31 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp32 = alloca [8 x i32], align 4
  store [8 x i32] %load_data31, ptr %arridxtmp32, align 4
  %arr_idx_ptr33 = getelementptr [8 x i32], ptr %arridxtmp32, i32 0, i32 %load_j30
  %arr_idx_load34 = load i32, ptr %arr_idx_ptr33, align 4
  %load_j35 = load i32, ptr %j_addr, align 4
  %addtmp36 = add i32 %load_j35, 1
  %load_data37 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp38 = alloca [8 x i32], align 4
  store [8 x i32] %load_data37, ptr %arridxtmp38, align 4
  %arr_idx_ptr39 = getelementptr [8 x i32], ptr %arridxtmp38, i32 0, i32 %addtmp36
  %arr_idx_load40 = load i32, ptr %arr_idx_ptr39, align 4
  %gttmp = icmp sgt i32 %arr_idx_load34, %arr_idx_load40
  br i1 %gttmp, label %then, label %ifcont

forpost23:                                        ; preds = %ifcont
  %load_j57 = load i32, ptr %j_addr, align 4
  %addtmp58 = add i32 %load_j57, 1
  store i32 %addtmp58, ptr %j_addr, align 4
  br label %forcond21

afterloop24:                                      ; preds = %forcond21
  br label %forpost15

ifcont:                                           ; preds = %then, %forbody22
  br label %forpost23

then:                                             ; preds = %forbody22
  %tmp_addr = alloca i32, align 4
  %load_j41 = load i32, ptr %j_addr, align 4
  %load_data42 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp43 = alloca [8 x i32], align 4
  store [8 x i32] %load_data42, ptr %arridxtmp43, align 4
  %arr_idx_ptr44 = getelementptr [8 x i32], ptr %arridxtmp43, i32 0, i32 %load_j41
  %arr_idx_load45 = load i32, ptr %arr_idx_ptr44, align 4
  store i32 %arr_idx_load45, ptr %tmp_addr, align 4
  %load_j46 = load i32, ptr %j_addr, align 4
  %load_j47 = load i32, ptr %j_addr, align 4
  %addtmp48 = add i32 %load_j47, 1
  %load_data49 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp50 = alloca [8 x i32], align 4
  store [8 x i32] %load_data49, ptr %arridxtmp50, align 4
  %arr_idx_ptr51 = getelementptr [8 x i32], ptr %arridxtmp50, i32 0, i32 %addtmp48
  %arr_idx_load52 = load i32, ptr %arr_idx_ptr51, align 4
  %arr_assign_ptr = getelementptr [8 x i32], ptr %data_addr, i32 0, i32 %load_j46
  store i32 %arr_idx_load52, ptr %arr_assign_ptr, align 4
  %load_j53 = load i32, ptr %j_addr, align 4
  %addtmp54 = add i32 %load_j53, 1
  %load_tmp = load i32, ptr %tmp_addr, align 4
  %arr_assign_ptr55 = getelementptr [8 x i32], ptr %data_addr, i32 0, i32 %addtmp54
  store i32 %load_tmp, ptr %arr_assign_ptr55, align 4
  %load_swaps = load i32, ptr %swaps_addr, align 4
  %addtmp56 = add i32 %load_swaps, 1
  store i32 %addtmp56, ptr %swaps_addr, align 4
  br label %ifcont

forcond63:                                        ; preds = %forpost65, %afterloop16
  %load_i67 = load i32, ptr %i_addr62, align 4
  %load_n68 = load i32, ptr %n_addr, align 4
  %lttmp69 = icmp slt i32 %load_i67, %load_n68
  br i1 %lttmp69, label %forbody64, label %afterloop66

forbody64:                                        ; preds = %forcond63
  %load_i70 = load i32, ptr %i_addr62, align 4
  %load_data71 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp72 = alloca [8 x i32], align 4
  store [8 x i32] %load_data71, ptr %arridxtmp72, align 4
  %arr_idx_ptr73 = getelementptr [8 x i32], ptr %arridxtmp72, i32 0, i32 %load_i70
  %arr_idx_load74 = load i32, ptr %arr_idx_ptr73, align 4
  %calltmp75 = call i32 (ptr, ...) @printf(ptr @str.4, i32 %arr_idx_load74)
  br label %forpost65

forpost65:                                        ; preds = %forbody64
  %load_i76 = load i32, ptr %i_addr62, align 4
  %addtmp77 = add i32 %load_i76, 1
  store i32 %addtmp77, ptr %i_addr62, align 4
  br label %forcond63

afterloop66:                                      ; preds = %forcond63
  %calltmp78 = call i32 (ptr, ...) @printf(ptr @str.5)
  %load_passes79 = load i32, ptr %passes_addr, align 4
  %calltmp80 = call i32 (ptr, ...) @printf(ptr @str.6, i32 %load_passes79)
  %load_swaps81 = load i32, ptr %swaps_addr, align 4
  %calltmp82 = call i32 (ptr, ...) @printf(ptr @str.7, i32 %load_swaps81)
  %load_data83 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp84 = alloca [8 x i32], align 4
  store [8 x i32] %load_data83, ptr %arridxtmp84, align 4
  %arr_idx_ptr85 = getelementptr [8 x i32], ptr %arridxtmp84, i32 0, i32 0
  %arr_idx_load86 = load i32, ptr %arr_idx_ptr85, align 4
  %load_data87 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp88 = alloca [8 x i32], align 4
  store [8 x i32] %load_data87, ptr %arridxtmp88, align 4
  %arr_idx_ptr89 = getelementptr [8 x i32], ptr %arridxtmp88, i32 0, i32 1
  %arr_idx_load90 = load i32, ptr %arr_idx_ptr89, align 4
  %calltmp91 = call i32 @min_of(i32 %arr_idx_load86, i32 %arr_idx_load90)
  %calltmp92 = call i32 (ptr, ...) @printf(ptr @str.8, i32 %calltmp91)
  %load_n93 = load i32, ptr %n_addr, align 4
  %subtmp94 = sub i32 %load_n93, 1
  %load_data95 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp96 = alloca [8 x i32], align 4
  store [8 x i32] %load_data95, ptr %arridxtmp96, align 4
  %arr_idx_ptr97 = getelementptr [8 x i32], ptr %arridxtmp96, i32 0, i32 %subtmp94
  %arr_idx_load98 = load i32, ptr %arr_idx_ptr97, align 4
  %load_n99 = load i32, ptr %n_addr, align 4
  %subtmp100 = sub i32 %load_n99, 2
  %load_data101 = load [8 x i32], ptr %data_addr, align 4
  %arridxtmp102 = alloca [8 x i32], align 4
  store [8 x i32] %load_data101, ptr %arridxtmp102, align 4
  %arr_idx_ptr103 = getelementptr [8 x i32], ptr %arridxtmp102, i32 0, i32 %subtmp100
  %arr_idx_load104 = load i32, ptr %arr_idx_ptr103, align 4
  %calltmp105 = call i32 @max_of(i32 %arr_idx_load98, i32 %arr_idx_load104)
  %calltmp106 = call i32 (ptr, ...) @printf(ptr @str.9, i32 %calltmp105)
  ret i32 0
}
