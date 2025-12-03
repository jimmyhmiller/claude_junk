; ModuleID = 'gc_test'
source_filename = "gc_test"

declare ptr addrspace(1) @gc_alloc(i64)

define i64 @gc_test_function() gc "statepoint-example" {
entry:
  %obj1 = call ptr addrspace(1) @gc_alloc(i64 64)
  store i64 3405691582, ptr addrspace(1) %obj1, align 4
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)
  %val = load i64, ptr addrspace(1) %obj1, align 4
  ret i64 %val
}
