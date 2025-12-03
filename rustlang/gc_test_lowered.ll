; ModuleID = 'gc_test_abstract.ll'
source_filename = "gc_test"

declare ptr addrspace(1) @gc_alloc(i64)

declare void @gc_safepoint(ptr)

define i64 @gc_test_function() gc "statepoint-example" {
entry:
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0)
  %obj11 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token)
  store i64 -3819410108451629448, ptr addrspace(1) %obj11, align 4
  %statepoint_token2 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj11) ]
  %obj23 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token2)
  %obj1.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token2, i32 0, i32 0) ; (%obj11, %obj11)
  store i64 -2401053088876216593, ptr addrspace(1) %obj23, align 4
  %statepoint_token4 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (i64)) @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %obj23, ptr addrspace(1) %obj1.relocated) ]
  %obj2.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token4, i32 0, i32 0) ; (%obj23, %obj23)
  %obj1.relocated5 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token4, i32 1, i32 1) ; (%obj1.relocated, %obj1.relocated)
  %val1 = load i64, ptr addrspace(1) %obj1.relocated5, align 4
  %val2 = load i64, ptr addrspace(1) %obj2.relocated, align 4
  %result = xor i64 %val1, %val2
  ret i64 %result
}

declare void @__tmp_use(...)

declare token @llvm.experimental.gc.statepoint.p0(i64 immarg, i32 immarg, ptr, i32 immarg, i32 immarg, ...)

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.result.p1(token) #0

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token, i32 immarg, i32 immarg) #0

attributes #0 = { nocallback nofree nosync nounwind willreturn memory(none) }
