; ModuleID = 'gc_test'
source_filename = "gc_test"

declare ptr addrspace(1) @gc_alloc(i64)

define i64 @test_gc() gc "statepoint-example" {
entry:
  %obj = call ptr addrspace(1) @gc_alloc(i64 64)
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)
  %val = load i64, ptr addrspace(1) %obj, align 4
  ret i64 %val
}

declare token @llvm.experimental.gc.statepoint.i64(i64 immarg, i32 immarg, i64, i32 immarg, i32 immarg, ...)

declare token @llvm.experimental.gc.statepoint.p1(i64 immarg, i32 immarg, ptr addrspace(1), i32 immarg, i32 immarg, ...)
