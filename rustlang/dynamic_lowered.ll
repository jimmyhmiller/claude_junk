; ModuleID = 'dynamic_abstract.ll'
source_filename = "dynamic_lang"

declare i64 @rt_cons(i64, i64)

define i1 @is_fixnum(i64 %0) {
entry:
  %tag = and i64 %0, 7
  %is_fix = icmp eq i64 %tag, 1
  ret i1 %is_fix
}

define i1 @is_heap_ptr(i64 %0) {
entry:
  %tag = and i64 %0, 7
  %is_ptr = icmp eq i64 %tag, 0
  %nonzero = icmp ne i64 %0, 0
  %is_heap = and i1 %is_ptr, %nonzero
  ret i1 %is_heap
}

define i64 @fixnum_add(i64 %0, i64 %1) {
entry:
  %a_val = ashr i64 %0, 3
  %b_val = ashr i64 %1, 3
  %sum = add i64 %a_val, %b_val
  %shifted = shl i64 %sum, 3
  %tagged = or i64 %shifted, 1
  ret i64 %tagged
}

define i64 @build_list_1_2_3() gc "statepoint-example" {
entry:
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64, i64)) @rt_cons, i32 2, i32 0, i64 25, i64 3, i32 0, i32 0)
  %cell31 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token)
  %statepoint_token2 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64, i64)) @rt_cons, i32 2, i32 0, i64 17, i64 %cell31, i32 0, i32 0)
  %cell23 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token2)
  %statepoint_token4 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64, i64)) @rt_cons, i32 2, i32 0, i64 9, i64 %cell23, i32 0, i32 0)
  %cell15 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token4)
  ret i64 %cell15
}

define i64 @type_dispatch(i64 %0) {
entry:
  %tag = and i64 %0, 7
  %is_fix = icmp eq i64 %tag, 1
  br i1 %is_fix, label %is_fixnum, label %is_heap

is_fixnum:                                        ; preds = %entry
  %v_val = ashr i64 %0, 3
  %inc = add i64 %v_val, 1
  %shifted = shl i64 %inc, 3
  %tagged = or i64 %shifted, 1
  br label %exit

is_heap:                                          ; preds = %entry
  %is_heap1 = icmp eq i64 %tag, 0
  br i1 %is_heap1, label %other, label %other

other:                                            ; preds = %is_heap, %is_heap
  br label %exit

exit:                                             ; preds = %other, %is_fixnum
  %result = phi i64 [ %tagged, %is_fixnum ], [ 3, %other ]
  ret i64 %result
}

declare token @llvm.experimental.gc.statepoint.p0(i64 immarg, i32 immarg, ptr, i32 immarg, i32 immarg, ...)

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare i64 @llvm.experimental.gc.result.i64(token) #0

attributes #0 = { nocallback nofree nosync nounwind willreturn memory(none) }
