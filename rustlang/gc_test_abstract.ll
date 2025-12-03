; ModuleID = 'gc_test'
source_filename = "gc_test"

declare ptr addrspace(1) @gc_alloc(i64)

declare void @gc_safepoint(ptr)

define i64 @gc_test_function() gc "statepoint-example" {
entry:
  %obj1 = call ptr addrspace(1) @gc_alloc(i64 64)
  store i64 -3819410108451629448, ptr addrspace(1) %obj1, align 4
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)
  store i64 -2401053088876216593, ptr addrspace(1) %obj2, align 4
  %obj3 = call ptr addrspace(1) @gc_alloc(i64 64)
  %val1 = load i64, ptr addrspace(1) %obj1, align 4
  %val2 = load i64, ptr addrspace(1) %obj2, align 4
  %result = xor i64 %val1, %val2
  ret i64 %result
}
