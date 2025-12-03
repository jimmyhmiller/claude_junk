; ModuleID = 'gc_test_abstract.ll'
source_filename = "gc_test"

declare ptr addrspace(1) @gc_alloc(i64)

define i64 @gc_test_function() gc "statepoint-example" {
entry:
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0)
  %obj11 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token)
  store i64 3405691582, ptr addrspace(1) %obj11, align 4
  %statepoint_token2 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj11) ]
  %obj1.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token2, i32 0, i32 0) ; (%obj11, %obj11)
  %val = load i64, ptr addrspace(1) %obj1.relocated, align 4
  ret i64 %val
}

declare void @__tmp_use(...)

declare token @llvm.experimental.gc.statepoint.p0(i64 immarg, i32 immarg, ptr, i32 immarg, i32 immarg, ...)

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.result.p1(token) #0

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token, i32 immarg, i32 immarg) #0

attributes #0 = { nocallback nofree nosync nounwind willreturn memory(none) }
