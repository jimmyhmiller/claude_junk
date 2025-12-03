; ModuleID = 'gc_test'
source_filename = "gc_test"

declare ptr addrspace(1) @gc_alloc(i64)

define i64 @test_function() gc "statepoint-example" {
entry:
  %obj1 = call ptr addrspace(1) @gc_alloc(i64 64)
  store i64 305419896, ptr addrspace(1) %obj1, align 4
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)
  %loaded = load i64, ptr addrspace(1) %obj1, align 4
  ret i64 %loaded
}
