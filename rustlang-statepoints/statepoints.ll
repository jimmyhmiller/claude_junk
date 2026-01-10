; ModuleID = 'lang_demo'
source_filename = "lang_demo"

declare ptr addrspace(1) @rt_cons_raw_mmtk(ptr addrspace(1), ptr addrspace(1))

declare ptr addrspace(1) @rt_try_alloc_cons(ptr addrspace(1), ptr addrspace(1))

declare ptr addrspace(1) @rt_car(ptr addrspace(1))

declare ptr addrspace(1) @rt_cdr(ptr addrspace(1))

declare void @rt_gc_mmtk()

declare ptr addrspace(1) @rt_cons_raw_mmtk_safepoint(ptr, ptr, i64, i64, i64)

declare void @rt_gc_with_frame_info(i64, i64, i64)

declare void @rt_gc_with_roots(i64, i64, i64, ptr addrspace(1), ptr addrspace(1))

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr @llvm.frameaddress.p0(i32 immarg) #0

; Function Attrs: nocallback nofree nosync nounwind willreturn
declare ptr @llvm.stacksave.p0() #1

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr @llvm.returnaddress(i32 immarg) #0

declare void @rt_print(i64)

declare void @rt_print_list(i64)

declare void @rt_print_stretch_check(i64, i64)

declare void @rt_print_trees_check(i64, i64, i64)

declare void @rt_print_long_lived_check(i64, i64)

declare void @rt_set_global_root(i64)

declare void @rt_clear_global_root()

declare i64 @rt_get_arg(i64)

declare i64 @rt_max(i64, i64)

define i64 @main() #2 gc "statepoint-example" {
entry:
  %iterations = alloca ptr addrspace(1), align 8
  %depth = alloca ptr addrspace(1), align 8
  %longLivedTree = alloca ptr addrspace(1), align 8
  %stretchDepth = alloca ptr addrspace(1), align 8
  %maxDepth = alloca ptr addrspace(1), align 8
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @rt_get_arg, i32 1, i32 0, i64 9, i32 0, i32 0)
  %get_arg77 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token)
  %to_gcptr = inttoptr i64 %get_arg77 to ptr addrspace(1)
  %to_i64 = ptrtoint ptr addrspace(1) %to_gcptr to i64
  %statepoint_token78 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64, i64)) @rt_max, i32 2, i32 0, i64 49, i64 %to_i64, i32 0, i32 0)
  %max79 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token78)
  %to_gcptr1 = inttoptr i64 %max79 to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr1, ptr %maxDepth, align 8
  %maxDepth2 = load ptr addrspace(1), ptr %maxDepth, align 8
  %to_i643 = ptrtoint ptr addrspace(1) %maxDepth2 to i64
  %sum = add i64 %to_i643, 9
  %fixnum_add = sub i64 %sum, 1
  %to_gcptr4 = inttoptr i64 %fixnum_add to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr4, ptr %stretchDepth, align 8
  %stretchDepth5 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %to_i646 = ptrtoint ptr addrspace(1) %stretchDepth5 to i64
  %stretchDepth7 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %to_i648 = ptrtoint ptr addrspace(1) %stretchDepth7 to i64
  %stretchDepth_live = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token80 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @bottom_up_tree, i32 1, i32 0, i64 %to_i648, i32 0, i32 0)
  %call81 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token80)
  %to_gcptr9 = inttoptr i64 %call81 to ptr addrspace(1)
  %to_i6410 = ptrtoint ptr addrspace(1) %to_gcptr9 to i64
  %stretchDepth_live11 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live12 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token82 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @item_check, i32 1, i32 0, i64 %to_i6410, i32 0, i32 0)
  %call1383 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token82)
  %to_gcptr14 = inttoptr i64 %call1383 to ptr addrspace(1)
  %to_i6415 = ptrtoint ptr addrspace(1) %to_gcptr14 to i64
  %stretchDepth_live16 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live17 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token84 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (i64, i64)) @rt_print_stretch_check, i32 2, i32 0, i64 %to_i646, i64 %to_i6415, i32 0, i32 0)
  %maxDepth18 = load ptr addrspace(1), ptr %maxDepth, align 8
  %to_i6419 = ptrtoint ptr addrspace(1) %maxDepth18 to i64
  %stretchDepth_live20 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live21 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token85 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @bottom_up_tree, i32 1, i32 0, i64 %to_i6419, i32 0, i32 0)
  %call2286 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token85)
  %to_gcptr23 = inttoptr i64 %call2286 to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr23, ptr %longLivedTree, align 8
  %longLivedTree24 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %to_i6425 = ptrtoint ptr addrspace(1) %longLivedTree24 to i64
  %longLivedTree_live = load ptr addrspace(1), ptr %longLivedTree, align 8
  %stretchDepth_live26 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live27 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token87 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (i64)) @rt_set_global_root, i32 1, i32 0, i64 %to_i6425, i32 0, i32 0)
  store ptr addrspace(1) inttoptr (i64 33 to ptr addrspace(1)), ptr %depth, align 8
  br label %whilecond

whilecond:                                        ; preds = %whilebody, %entry
  %depth28 = load ptr addrspace(1), ptr %depth, align 8
  %to_i6429 = ptrtoint ptr addrspace(1) %depth28 to i64
  %maxDepth30 = load ptr addrspace(1), ptr %maxDepth, align 8
  %to_i6431 = ptrtoint ptr addrspace(1) %maxDepth30 to i64
  %le = icmp sle i64 %to_i6429, %to_i6431
  %le_result = select i1 %le, ptr addrspace(1) inttoptr (i64 11 to ptr addrspace(1)), ptr addrspace(1) inttoptr (i64 19 to ptr addrspace(1))
  %to_i6432 = ptrtoint ptr addrspace(1) %le_result to i64
  %whilecond33 = icmp ne i64 %to_i6432, 19
  br i1 %whilecond33, label %whilebody, label %whileend

whilebody:                                        ; preds = %whilecond
  %maxDepth34 = load ptr addrspace(1), ptr %maxDepth, align 8
  %to_i6435 = ptrtoint ptr addrspace(1) %maxDepth34 to i64
  %depth36 = load ptr addrspace(1), ptr %depth, align 8
  %to_i6437 = ptrtoint ptr addrspace(1) %depth36 to i64
  %diff = sub i64 %to_i6435, %to_i6437
  %fixnum_sub = add i64 %diff, 1
  %to_gcptr38 = inttoptr i64 %fixnum_sub to ptr addrspace(1)
  %to_i6439 = ptrtoint ptr addrspace(1) %to_gcptr38 to i64
  %sum40 = add i64 %to_i6439, 33
  %fixnum_add41 = sub i64 %sum40, 1
  %to_gcptr42 = inttoptr i64 %fixnum_add41 to ptr addrspace(1)
  %to_i6443 = ptrtoint ptr addrspace(1) %to_gcptr42 to i64
  %b_untag = lshr i64 %to_i6443, 3
  %shl = shl i64 1, %b_untag
  %result_shift = shl i64 %shl, 3
  %fixnum_shl = or i64 %result_shift, 1
  %to_gcptr44 = inttoptr i64 %fixnum_shl to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr44, ptr %iterations, align 8
  %iterations45 = load ptr addrspace(1), ptr %iterations, align 8
  %to_i6446 = ptrtoint ptr addrspace(1) %iterations45 to i64
  %depth47 = load ptr addrspace(1), ptr %depth, align 8
  %to_i6448 = ptrtoint ptr addrspace(1) %depth47 to i64
  %longLivedTree49 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %to_i6450 = ptrtoint ptr addrspace(1) %longLivedTree49 to i64
  %longLivedTree_live51 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %stretchDepth_live52 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live53 = load ptr addrspace(1), ptr %maxDepth, align 8
  %depth_live = load ptr addrspace(1), ptr %depth, align 8
  %iterations_live = load ptr addrspace(1), ptr %iterations, align 8
  %statepoint_token88 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64, i64, i64)) @work, i32 3, i32 0, i64 %to_i6446, i64 %to_i6448, i64 %to_i6450, i32 0, i32 0)
  %call5489 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token88)
  %to_gcptr55 = inttoptr i64 %call5489 to ptr addrspace(1)
  %depth56 = load ptr addrspace(1), ptr %depth, align 8
  %to_i6457 = ptrtoint ptr addrspace(1) %depth56 to i64
  %sum58 = add i64 %to_i6457, 17
  %fixnum_add59 = sub i64 %sum58, 1
  %to_gcptr60 = inttoptr i64 %fixnum_add59 to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr60, ptr %depth, align 8
  br label %whilecond

whileend:                                         ; preds = %whilecond
  %maxDepth61 = load ptr addrspace(1), ptr %maxDepth, align 8
  %to_i6462 = ptrtoint ptr addrspace(1) %maxDepth61 to i64
  %longLivedTree63 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %to_i6464 = ptrtoint ptr addrspace(1) %longLivedTree63 to i64
  %longLivedTree_live65 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %stretchDepth_live66 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live67 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token90 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @item_check, i32 1, i32 0, i64 %to_i6464, i32 0, i32 0)
  %call6891 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token90)
  %to_gcptr69 = inttoptr i64 %call6891 to ptr addrspace(1)
  %to_i6470 = ptrtoint ptr addrspace(1) %to_gcptr69 to i64
  %longLivedTree_live71 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %stretchDepth_live72 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live73 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token92 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (i64, i64)) @rt_print_long_lived_check, i32 2, i32 0, i64 %to_i6462, i64 %to_i6470, i32 0, i32 0)
  %longLivedTree_live74 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %stretchDepth_live75 = load ptr addrspace(1), ptr %stretchDepth, align 8
  %maxDepth_live76 = load ptr addrspace(1), ptr %maxDepth, align 8
  %statepoint_token93 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void ()) @rt_clear_global_root, i32 0, i32 0, i32 0, i32 0)
  ret i64 3
}

define i64 @bottom_up_tree(i64 %0) #2 gc "statepoint-example" {
entry:
  %cdr_slot20 = alloca ptr addrspace(1), align 8
  %car_slot19 = alloca ptr addrspace(1), align 8
  %cdr_slot = alloca ptr addrspace(1), align 8
  %car_slot = alloca ptr addrspace(1), align 8
  %to_gcptr = inttoptr i64 %0 to ptr addrspace(1)
  %depth = alloca ptr addrspace(1), align 8
  store ptr addrspace(1) %to_gcptr, ptr %depth, align 8
  %depth1 = load ptr addrspace(1), ptr %depth, align 8
  %to_i64 = ptrtoint ptr addrspace(1) %depth1 to i64
  %gt = icmp sgt i64 %to_i64, 1
  %gt_result = select i1 %gt, ptr addrspace(1) inttoptr (i64 11 to ptr addrspace(1)), ptr addrspace(1) inttoptr (i64 19 to ptr addrspace(1))
  %to_i642 = ptrtoint ptr addrspace(1) %gt_result to i64
  %ifcond = icmp ne i64 %to_i642, 19
  br i1 %ifcond, label %then, label %else

then:                                             ; preds = %entry
  %depth3 = load ptr addrspace(1), ptr %depth, align 8
  %to_i644 = ptrtoint ptr addrspace(1) %depth3 to i64
  %diff = sub i64 %to_i644, 9
  %fixnum_sub = add i64 %diff, 1
  %to_gcptr5 = inttoptr i64 %fixnum_sub to ptr addrspace(1)
  %to_i646 = ptrtoint ptr addrspace(1) %to_gcptr5 to i64
  %depth_live = load ptr addrspace(1), ptr %depth, align 8
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @bottom_up_tree, i32 1, i32 0, i64 %to_i646, i32 0, i32 0)
  %call35 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token)
  %to_gcptr7 = inttoptr i64 %call35 to ptr addrspace(1)
  %depth8 = load ptr addrspace(1), ptr %depth, align 8
  %to_i649 = ptrtoint ptr addrspace(1) %depth8 to i64
  %diff10 = sub i64 %to_i649, 9
  %fixnum_sub11 = add i64 %diff10, 1
  %to_gcptr12 = inttoptr i64 %fixnum_sub11 to ptr addrspace(1)
  %to_i6413 = ptrtoint ptr addrspace(1) %to_gcptr12 to i64
  %depth_live14 = load ptr addrspace(1), ptr %depth, align 8
  %statepoint_token36 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @bottom_up_tree, i32 1, i32 0, i64 %to_i6413, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %to_gcptr7) ]
  %call1537 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token36)
  %to_gcptr7.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token36, i32 0, i32 0) ; (%to_gcptr7, %to_gcptr7)
  %to_gcptr16 = inttoptr i64 %call1537 to ptr addrspace(1)
  %depth_live17 = load ptr addrspace(1), ptr %depth, align 8
  store ptr addrspace(1) %to_gcptr7.relocated, ptr %car_slot, align 8
  store ptr addrspace(1) %to_gcptr16, ptr %cdr_slot, align 8
  br label %cons_retry

else:                                             ; preds = %entry
  %depth_live18 = load ptr addrspace(1), ptr %depth, align 8
  store ptr addrspace(1) inttoptr (i64 3 to ptr addrspace(1)), ptr %car_slot19, align 8
  store ptr addrspace(1) inttoptr (i64 3 to ptr addrspace(1)), ptr %cdr_slot20, align 8
  br label %cons_retry21

ifcont:                                           ; preds = %cons_done23, %cons_done
  %ifresult = phi ptr addrspace(1) [ %try_cons39, %cons_done ], [ %try_cons2644, %cons_done23 ]
  %to_i6434 = ptrtoint ptr addrspace(1) %ifresult to i64
  ret i64 %to_i6434

cons_retry:                                       ; preds = %cons_alloc, %then
  %car_val = load ptr addrspace(1), ptr %car_slot, align 8
  %cdr_val = load ptr addrspace(1), ptr %cdr_slot, align 8
  %statepoint_token38 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (ptr addrspace(1), ptr addrspace(1))) @rt_try_alloc_cons, i32 2, i32 0, ptr addrspace(1) %car_val, ptr addrspace(1) %cdr_val, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %cdr_val, ptr addrspace(1) %car_val) ]
  %try_cons39 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token38)
  %cdr_val.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token38, i32 0, i32 0) ; (%cdr_val, %cdr_val)
  %car_val.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token38, i32 1, i32 1) ; (%car_val, %car_val)
  %is_null = icmp eq ptr addrspace(1) %try_cons39, null
  br i1 %is_null, label %cons_alloc, label %cons_done

cons_alloc:                                       ; preds = %cons_retry
  %fp_ptr = call ptr @llvm.frameaddress.p0(i32 0)
  %fp = ptrtoint ptr %fp_ptr to i64
  %sp_ptr = call ptr @llvm.stacksave.p0()
  %sp = ptrtoint ptr %sp_ptr to i64
  %ra_ptr = call ptr @llvm.returnaddress(i32 0)
  %ra = ptrtoint ptr %ra_ptr to i64
  %statepoint_token40 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (i64, i64, i64, ptr addrspace(1), ptr addrspace(1))) @rt_gc_with_roots, i32 5, i32 0, i64 %fp, i64 %sp, i64 %ra, ptr addrspace(1) %car_val.relocated, ptr addrspace(1) %cdr_val.relocated, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %cdr_val.relocated, ptr addrspace(1) %car_val.relocated) ]
  %cdr_val.relocated41 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token40, i32 0, i32 0) ; (%cdr_val.relocated, %cdr_val.relocated)
  %car_val.relocated42 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token40, i32 1, i32 1) ; (%car_val.relocated, %car_val.relocated)
  store ptr addrspace(1) %car_val.relocated42, ptr %car_slot, align 8
  store ptr addrspace(1) %cdr_val.relocated41, ptr %cdr_slot, align 8
  br label %cons_retry

cons_done:                                        ; preds = %cons_retry
  br label %ifcont

cons_retry21:                                     ; preds = %cons_alloc22, %else
  %car_val24 = load ptr addrspace(1), ptr %car_slot19, align 8
  %cdr_val25 = load ptr addrspace(1), ptr %cdr_slot20, align 8
  %statepoint_token43 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (ptr addrspace(1), ptr addrspace(1))) @rt_try_alloc_cons, i32 2, i32 0, ptr addrspace(1) %car_val24, ptr addrspace(1) %cdr_val25, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %cdr_val25, ptr addrspace(1) %car_val24) ]
  %try_cons2644 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token43)
  %cdr_val25.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token43, i32 0, i32 0) ; (%cdr_val25, %cdr_val25)
  %car_val24.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token43, i32 1, i32 1) ; (%car_val24, %car_val24)
  %is_null27 = icmp eq ptr addrspace(1) %try_cons2644, null
  br i1 %is_null27, label %cons_alloc22, label %cons_done23

cons_alloc22:                                     ; preds = %cons_retry21
  %fp_ptr28 = call ptr @llvm.frameaddress.p0(i32 0)
  %fp29 = ptrtoint ptr %fp_ptr28 to i64
  %sp_ptr30 = call ptr @llvm.stacksave.p0()
  %sp31 = ptrtoint ptr %sp_ptr30 to i64
  %ra_ptr32 = call ptr @llvm.returnaddress(i32 0)
  %ra33 = ptrtoint ptr %ra_ptr32 to i64
  %statepoint_token45 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (i64, i64, i64, ptr addrspace(1), ptr addrspace(1))) @rt_gc_with_roots, i32 5, i32 0, i64 %fp29, i64 %sp31, i64 %ra33, ptr addrspace(1) %car_val24.relocated, ptr addrspace(1) %cdr_val25.relocated, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %cdr_val25.relocated, ptr addrspace(1) %car_val24.relocated) ]
  %cdr_val25.relocated46 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token45, i32 0, i32 0) ; (%cdr_val25.relocated, %cdr_val25.relocated)
  %car_val24.relocated47 = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token45, i32 1, i32 1) ; (%car_val24.relocated, %car_val24.relocated)
  store ptr addrspace(1) %car_val24.relocated47, ptr %car_slot19, align 8
  store ptr addrspace(1) %cdr_val25.relocated46, ptr %cdr_slot20, align 8
  br label %cons_retry21

cons_done23:                                      ; preds = %cons_retry21
  br label %ifcont
}

define i64 @item_check(i64 %0) #2 gc "statepoint-example" {
entry:
  %to_gcptr = inttoptr i64 %0 to ptr addrspace(1)
  %node = alloca ptr addrspace(1), align 8
  store ptr addrspace(1) %to_gcptr, ptr %node, align 8
  %node1 = load ptr addrspace(1), ptr %node, align 8
  %node_live = load ptr addrspace(1), ptr %node, align 8
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (ptr addrspace(1))) @rt_car, i32 1, i32 0, ptr addrspace(1) %node1, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %node1) ]
  %car23 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token)
  %node1.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token, i32 0, i32 0) ; (%node1, %node1)
  %to_i64 = ptrtoint ptr addrspace(1) %car23 to i64
  %nullp = icmp eq i64 %to_i64, 3
  %nullp_result = select i1 %nullp, ptr addrspace(1) inttoptr (i64 11 to ptr addrspace(1)), ptr addrspace(1) inttoptr (i64 19 to ptr addrspace(1))
  %to_i642 = ptrtoint ptr addrspace(1) %nullp_result to i64
  %ifcond = icmp ne i64 %to_i642, 19
  br i1 %ifcond, label %then, label %else

then:                                             ; preds = %entry
  br label %ifcont

else:                                             ; preds = %entry
  %node3 = load ptr addrspace(1), ptr %node, align 8
  %node_live4 = load ptr addrspace(1), ptr %node, align 8
  %statepoint_token24 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (ptr addrspace(1))) @rt_car, i32 1, i32 0, ptr addrspace(1) %node3, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %node3) ]
  %car525 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token24)
  %node3.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token24, i32 0, i32 0) ; (%node3, %node3)
  %to_i646 = ptrtoint ptr addrspace(1) %car525 to i64
  %node_live7 = load ptr addrspace(1), ptr %node, align 8
  %statepoint_token26 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @item_check, i32 1, i32 0, i64 %to_i646, i32 0, i32 0)
  %call27 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token26)
  %to_gcptr8 = inttoptr i64 %call27 to ptr addrspace(1)
  %to_i649 = ptrtoint ptr addrspace(1) %to_gcptr8 to i64
  %node10 = load ptr addrspace(1), ptr %node, align 8
  %node_live11 = load ptr addrspace(1), ptr %node, align 8
  %statepoint_token28 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(ptr addrspace(1) (ptr addrspace(1))) @rt_cdr, i32 1, i32 0, ptr addrspace(1) %node10, i32 0, i32 0) [ "gc-live"(ptr addrspace(1) %node10) ]
  %cdr29 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %statepoint_token28)
  %node10.relocated = call coldcc ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token %statepoint_token28, i32 0, i32 0) ; (%node10, %node10)
  %to_i6412 = ptrtoint ptr addrspace(1) %cdr29 to i64
  %node_live13 = load ptr addrspace(1), ptr %node, align 8
  %statepoint_token30 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @item_check, i32 1, i32 0, i64 %to_i6412, i32 0, i32 0)
  %call1431 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token30)
  %to_gcptr15 = inttoptr i64 %call1431 to ptr addrspace(1)
  %to_i6416 = ptrtoint ptr addrspace(1) %to_gcptr15 to i64
  %sum = add i64 %to_i649, %to_i6416
  %fixnum_add = sub i64 %sum, 1
  %to_gcptr17 = inttoptr i64 %fixnum_add to ptr addrspace(1)
  %to_i6418 = ptrtoint ptr addrspace(1) %to_gcptr17 to i64
  %sum19 = add i64 9, %to_i6418
  %fixnum_add20 = sub i64 %sum19, 1
  %to_gcptr21 = inttoptr i64 %fixnum_add20 to ptr addrspace(1)
  br label %ifcont

ifcont:                                           ; preds = %else, %then
  %ifresult = phi ptr addrspace(1) [ inttoptr (i64 9 to ptr addrspace(1)), %then ], [ %to_gcptr21, %else ]
  %to_i6422 = ptrtoint ptr addrspace(1) %ifresult to i64
  ret i64 %to_i6422
}

define i64 @work(i64 %0, i64 %1, i64 %2) #2 gc "statepoint-example" {
entry:
  %i = alloca ptr addrspace(1), align 8
  %check = alloca ptr addrspace(1), align 8
  %to_gcptr = inttoptr i64 %0 to ptr addrspace(1)
  %iterations = alloca ptr addrspace(1), align 8
  store ptr addrspace(1) %to_gcptr, ptr %iterations, align 8
  %to_gcptr1 = inttoptr i64 %1 to ptr addrspace(1)
  %depth = alloca ptr addrspace(1), align 8
  store ptr addrspace(1) %to_gcptr1, ptr %depth, align 8
  %to_gcptr2 = inttoptr i64 %2 to ptr addrspace(1)
  %longLivedTree = alloca ptr addrspace(1), align 8
  store ptr addrspace(1) %to_gcptr2, ptr %longLivedTree, align 8
  store ptr addrspace(1) inttoptr (i64 1 to ptr addrspace(1)), ptr %check, align 8
  store ptr addrspace(1) inttoptr (i64 1 to ptr addrspace(1)), ptr %i, align 8
  br label %whilecond

whilecond:                                        ; preds = %whilebody, %entry
  %i3 = load ptr addrspace(1), ptr %i, align 8
  %to_i64 = ptrtoint ptr addrspace(1) %i3 to i64
  %iterations4 = load ptr addrspace(1), ptr %iterations, align 8
  %to_i645 = ptrtoint ptr addrspace(1) %iterations4 to i64
  %lt = icmp slt i64 %to_i64, %to_i645
  %lt_result = select i1 %lt, ptr addrspace(1) inttoptr (i64 11 to ptr addrspace(1)), ptr addrspace(1) inttoptr (i64 19 to ptr addrspace(1))
  %to_i646 = ptrtoint ptr addrspace(1) %lt_result to i64
  %whilecond7 = icmp ne i64 %to_i646, 19
  br i1 %whilecond7, label %whilebody, label %whileend

whilebody:                                        ; preds = %whilecond
  %check8 = load ptr addrspace(1), ptr %check, align 8
  %to_i649 = ptrtoint ptr addrspace(1) %check8 to i64
  %depth10 = load ptr addrspace(1), ptr %depth, align 8
  %to_i6411 = ptrtoint ptr addrspace(1) %depth10 to i64
  %check_live = load ptr addrspace(1), ptr %check, align 8
  %iterations_live = load ptr addrspace(1), ptr %iterations, align 8
  %i_live = load ptr addrspace(1), ptr %i, align 8
  %depth_live = load ptr addrspace(1), ptr %depth, align 8
  %longLivedTree_live = load ptr addrspace(1), ptr %longLivedTree, align 8
  %statepoint_token = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @bottom_up_tree, i32 1, i32 0, i64 %to_i6411, i32 0, i32 0)
  %call39 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token)
  %to_gcptr12 = inttoptr i64 %call39 to ptr addrspace(1)
  %to_i6413 = ptrtoint ptr addrspace(1) %to_gcptr12 to i64
  %check_live14 = load ptr addrspace(1), ptr %check, align 8
  %iterations_live15 = load ptr addrspace(1), ptr %iterations, align 8
  %i_live16 = load ptr addrspace(1), ptr %i, align 8
  %depth_live17 = load ptr addrspace(1), ptr %depth, align 8
  %longLivedTree_live18 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %statepoint_token40 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(i64 (i64)) @item_check, i32 1, i32 0, i64 %to_i6413, i32 0, i32 0)
  %call1941 = call i64 @llvm.experimental.gc.result.i64(token %statepoint_token40)
  %to_gcptr20 = inttoptr i64 %call1941 to ptr addrspace(1)
  %to_i6421 = ptrtoint ptr addrspace(1) %to_gcptr20 to i64
  %sum = add i64 %to_i649, %to_i6421
  %fixnum_add = sub i64 %sum, 1
  %to_gcptr22 = inttoptr i64 %fixnum_add to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr22, ptr %check, align 8
  %i23 = load ptr addrspace(1), ptr %i, align 8
  %to_i6424 = ptrtoint ptr addrspace(1) %i23 to i64
  %sum25 = add i64 %to_i6424, 9
  %fixnum_add26 = sub i64 %sum25, 1
  %to_gcptr27 = inttoptr i64 %fixnum_add26 to ptr addrspace(1)
  store ptr addrspace(1) %to_gcptr27, ptr %i, align 8
  br label %whilecond

whileend:                                         ; preds = %whilecond
  %iterations28 = load ptr addrspace(1), ptr %iterations, align 8
  %to_i6429 = ptrtoint ptr addrspace(1) %iterations28 to i64
  %depth30 = load ptr addrspace(1), ptr %depth, align 8
  %to_i6431 = ptrtoint ptr addrspace(1) %depth30 to i64
  %check32 = load ptr addrspace(1), ptr %check, align 8
  %to_i6433 = ptrtoint ptr addrspace(1) %check32 to i64
  %check_live34 = load ptr addrspace(1), ptr %check, align 8
  %iterations_live35 = load ptr addrspace(1), ptr %iterations, align 8
  %i_live36 = load ptr addrspace(1), ptr %i, align 8
  %depth_live37 = load ptr addrspace(1), ptr %depth, align 8
  %longLivedTree_live38 = load ptr addrspace(1), ptr %longLivedTree, align 8
  %statepoint_token42 = call token (i64, i32, ptr, i32, i32, ...) @llvm.experimental.gc.statepoint.p0(i64 2882400000, i32 0, ptr elementtype(void (i64, i64, i64)) @rt_print_trees_check, i32 3, i32 0, i64 %to_i6429, i64 %to_i6431, i64 %to_i6433, i32 0, i32 0)
  ret i64 3
}

declare token @llvm.experimental.gc.statepoint.p0(i64 immarg, i32 immarg, ptr, i32 immarg, i32 immarg, ...)

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare i64 @llvm.experimental.gc.result.i64(token) #0

declare void @__tmp_use(...)

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.relocate.p1(token, i32 immarg, i32 immarg) #0

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(none)
declare ptr addrspace(1) @llvm.experimental.gc.result.p1(token) #0

attributes #0 = { nocallback nofree nosync nounwind willreturn memory(none) }
attributes #1 = { nocallback nofree nosync nounwind willreturn }
attributes #2 = { "frame-pointer"="all" }
