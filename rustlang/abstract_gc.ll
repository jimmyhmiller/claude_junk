; ModuleID = 'gc_demo'
source_filename = "gc_demo"

declare ptr addrspace(1) @gc_alloc(i64)

declare void @use_object(ptr addrspace(1))

define i64 @simple_gc_example() gc "statepoint-example" {
entry:
  %obj1 = call ptr addrspace(1) @gc_alloc(i64 64)
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)
  call void @use_object(ptr addrspace(1) %obj1)
  %val = load i64, ptr addrspace(1) %obj1, align 4
  ret i64 %val
}

define void @loop_gc_example(i32 %0) gc "statepoint-example" {
entry:
  %initial_obj = call ptr addrspace(1) @gc_alloc(i64 32)
  br label %loop.header

loop.header:                                      ; preds = %loop.body, %entry
  %i = phi i32 [ 0, %entry ], [ %i.next, %loop.body ]
  %obj = phi ptr addrspace(1) [ %initial_obj, %entry ], [ %new_obj, %loop.body ]
  %cond = icmp slt i32 %i, %0
  br i1 %cond, label %loop.body, label %exit

loop.body:                                        ; preds = %loop.header
  call void @use_object(ptr addrspace(1) %obj)
  %new_obj = call ptr addrspace(1) @gc_alloc(i64 32)
  %i.next = add i32 %i, 1
  br label %loop.header

exit:                                             ; preds = %loop.header
  ret void
}

define i64 @multi_pointer_example() gc "statepoint-example" {
entry:
  %obj1 = call ptr addrspace(1) @gc_alloc(i64 64)
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)
  %obj3 = call ptr addrspace(1) @gc_alloc(i64 64)
  %obj4 = call ptr addrspace(1) @gc_alloc(i64 64)
  %v1 = load i64, ptr addrspace(1) %obj1, align 4
  %v2 = load i64, ptr addrspace(1) %obj2, align 4
  %v3 = load i64, ptr addrspace(1) %obj3, align 4
  %sum1 = add i64 %v1, %v2
  %sum2 = add i64 %sum1, %v3
  ret i64 %sum2
}
