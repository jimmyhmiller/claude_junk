; ModuleID = 'jit_demo'
source_filename = "jit_demo"

declare i64 @rt_cons(i64, i64)

declare void @rt_gc()

declare void @gc_shadow_stack_push(ptr, i32)

declare void @gc_shadow_stack_pop(ptr)

define i64 @build_list() {
entry:
  %frame = alloca { ptr, i32, [32 x i64] }, align 8
  call void @gc_shadow_stack_push(ptr %frame, i32 2)
  %cell3 = call i64 @rt_cons(i64 25, i64 3)
  %slots_ptr = getelementptr inbounds nuw { ptr, i32, [32 x i64] }, ptr %frame, i32 0, i32 2
  %slot0 = getelementptr [32 x i64], ptr %slots_ptr, i32 0, i32 0
  store i64 %cell3, ptr %slot0, align 4
  call void @rt_gc()
  %cell3_rel = load i64, ptr %slot0, align 4
  %cell2 = call i64 @rt_cons(i64 17, i64 %cell3_rel)
  %slot1 = getelementptr [32 x i64], ptr %slots_ptr, i32 0, i32 1
  store i64 %cell2, ptr %slot1, align 4
  call void @rt_gc()
  %cell2_rel = load i64, ptr %slot1, align 4
  %cell1 = call i64 @rt_cons(i64 9, i64 %cell2_rel)
  call void @gc_shadow_stack_pop(ptr %frame)
  ret i64 %cell1
}
