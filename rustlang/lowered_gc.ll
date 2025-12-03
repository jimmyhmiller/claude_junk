; ModuleID = 'abstract_gc.ll'
source_filename = "gc_demo"

declare ptr addrspace(1) @gc_alloc(i64)

declare void @use_object(ptr addrspace(1))

define i64 @simple_gc_example() gc "statepoint-example" {
entry:
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0)
  %obj11 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token)
  %statepoint_token2 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj11) ]
  %obj1.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token2, i32 0, i32 0) ; (%obj11, %obj11)
  %statepoint_token3 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (ptr addrspace(1))) @use_object, i32 1, i32 0, ptr addrspace(1) %obj1.relocated, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj1.relocated) ]
  %obj1.relocated4 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token3, i32 0, i32 0) ; (%obj1.relocated, %obj1.relocated)
  %val = load i64, ptr addrspace(1) %obj1.relocated4, align 4
  ret i64 %val
}

define void @loop_gc_example(i32 %0) gc "statepoint-example" {
entry:
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 32, i32 0, i32 0)
  %initial_obj1 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token)
  br label %loop.header

loop.header:                                      ; preds = %loop.body, %entry
  %i = phi i32 [ 0, %entry ], [ %i.next, %loop.body ]
  %obj = phi ptr addrspace(1) [ %initial_obj1, %entry ], [ %new_obj4, %loop.body ]
  %cond = icmp slt i32 %i, %0
  br i1 %cond, label %loop.body, label %exit

loop.body:                                        ; preds = %loop.header
  %statepoint_token2 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (ptr addrspace(1))) @use_object, i32 1, i32 0, ptr addrspace(1) %obj, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj) ]
  %obj.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token2, i32 0, i32 0) ; (%obj, %obj)
  %statepoint_token3 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 32, i32 0, i32 0)
  %new_obj4 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token3)
  %i.next = add i32 %i, 1
  br label %loop.header

exit:                                             ; preds = %loop.header
  ret void
}

define i64 @multi_pointer_example() gc "statepoint-example" {
entry:
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0)
  %obj11 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token)
  %statepoint_token2 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj11) ]
  %obj23 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token2)
  %obj1.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token2, i32 0, i32 0) ; (%obj11, %obj11)
  %statepoint_token4 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj23, ptr addrspace(1) %obj1.relocated) ]
  %obj35 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token4)
  %obj2.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token4, i32 0, i32 0) ; (%obj23, %obj23)
  %obj1.relocated6 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token4, i32 1, i32 1) ; (%obj1.relocated, %obj1.relocated)
  %statepoint_token7 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj35, ptr addrspace(1) %obj2.relocated, ptr addrspace(1) %obj1.relocated6) ]
  %obj3.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token7, i32 0, i32 0) ; (%obj35, %obj35)
  %obj2.relocated8 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token7, i32 1, i32 1) ; (%obj2.relocated, %obj2.relocated)
  %obj1.relocated9 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token7, i32 2, i32 2) ; (%obj1.relocated6, %obj1.relocated6)
  %v1 = load i64, ptr addrspace(1) %obj1.relocated9, align 4
  %v2 = load i64, ptr addrspace(1) %obj2.relocated8, align 4
  %v3 = load i64, ptr addrspace(1) %obj3.relocated, align 4
  %sum1 = add i64 %v1, %v2
  %sum2 = add i64 %sum1, %v3
  ret i64 %sum2
}

declare void @__tmp_use(...)

declare token @llvm.experimental.gc.statepoint.p0(i64 immarg, i32 immarg, ptr, i32 immarg, i32 immarg, ...)

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.result.p1(token) #0

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token, i32 immarg, i32 immarg) #0

attributes #0 = { nocallback nofree nosync nounwind willreturn memory(none) }
