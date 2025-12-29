# Building a Language with LLVM Statepoints and GC

This guide shows you how to build a garbage-collected language using LLVM's statepoint infrastructure. It's based on the working proof-of-concept in this repo.

## What Are Statepoints?

Statepoints are LLVM's mechanism for precise garbage collection. At each point where GC might occur (allocation, safepoint), LLVM:
1. Records which GC pointers are live on the stack
2. Generates stack maps showing where those pointers are (registers, stack slots)
3. Allows your GC to update those pointers when objects move

## Architecture Overview

```
Your Language Source
       ↓
    Parser/Frontend
       ↓
    LLVM IR (with GC markers)
       ↓
    opt -passes=rewrite-statepoints-for-gc
       ↓
    LLVM IR (with statepoints)
       ↓
    Compile to object file
       ↓
    Link with GC runtime
       ↓
    Executable
```

## Step 1: Set Up Your GC Runtime

You need a garbage collector that can:
- Allocate objects
- Copy/move objects during collection
- Update pointers to moved objects

**Key files to study:**
- `rustlang/src/gc_runtime.rs` - Semi-space copying collector
- Focus on the `collect()` function (line 192)

**Essential GC API:**
```c
// Initialize GC heap
void gc_init();

// Allocate object (this is a safepoint!)
void* gc_alloc(size_t size);

// Called by LLVM-generated code at safepoints
// Roots are stack locations containing GC pointers
void gc_safepoint(void** roots, size_t num_roots);
```

**Critical insight:** Your GC must be able to:
1. Find all live pointers (LLVM gives you this via stack maps)
2. Update those pointers in-place when objects move
3. Handle the "root" set from the stack

## Step 2: Generate LLVM IR with GC Annotations

When generating LLVM IR, you must:

### 2.1: Mark GC Pointers with Address Space 1

```llvm
; Regular pointer
%regular = alloca i64

; GC pointer - uses addrspace(1)
%gc_obj = call ptr addrspace(1) @gc_alloc(i64 64)
```

All GC-managed pointers MUST use `addrspace(1)`. This tells LLVM which pointers need tracking.

### 2.2: Mark Functions with GC Strategy

```llvm
define i64 @my_function() gc "statepoint-example" {
  ; ... your code
}
```

The `gc "statepoint-example"` attribute tells LLVM to insert statepoints.

### 2.3: Example IR Before Statepoint Lowering

```llvm
define i64 @test() gc "statepoint-example" {
entry:
  ; Allocate first object
  %obj1 = call ptr addrspace(1) @gc_alloc(i64 64)

  ; Store value
  store i64 42, ptr addrspace(1) %obj1

  ; Second allocation - obj1 might move during this!
  %obj2 = call ptr addrspace(1) @gc_alloc(i64 64)

  ; Load from obj1 - needs relocated pointer
  %val = load i64, ptr addrspace(1) %obj1
  ret i64 %val
}
```

**See:** `rustlang/src/main.rs:183-211` for how to generate this with inkwell.

## Step 3: Run the Statepoint Rewrite Pass

After generating IR, run LLVM's optimization pass:

```bash
opt -passes=rewrite-statepoints-for-gc input.ll -S -o output.ll
```

This transforms your code to insert statepoints and relocations.

### What Changes?

**Before:**
```llvm
%obj1 = call ptr addrspace(1) @gc_alloc(i64 64)
store i64 42, ptr addrspace(1) %obj1
%obj2 = call ptr addrspace(1) @gc_alloc(i64 64)  ; obj1 might move!
%val = load i64, ptr addrspace(1) %obj1         ; uses old pointer!
```

**After:**
```llvm
; First allocation wrapped in statepoint
%token1 = call token @llvm.experimental.gc.statepoint.p0(
    i64 2882400000, i32 0, ptr @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0)
%obj1 = call ptr addrspace(1) @llvm.experimental.gc.result.p1(token %token1)

store i64 42, ptr addrspace(1) %obj1

; Second allocation - declares obj1 as "gc-live"
%token2 = call token @llvm.experimental.gc.statepoint.p0(
    i64 2882400000, i32 0, ptr @gc_alloc, i32 1, i32 0, i64 64, i32 0, i32 0)
    [ "gc-live"(ptr addrspace(1) %obj1) ]

; Get relocated pointer (might be different address!)
%obj1.relocated = call ptr addrspace(1) @llvm.experimental.gc.relocate.p1(
    token %token2, i32 0, i32 0)

; Load using relocated pointer
%val = load i64, ptr addrspace(1) %obj1.relocated
```

**Key point:** `gc.relocate` gives you the updated pointer after GC.

**See:** `rustlang/gc_test_lowered.ll` for real output.

## Step 4: Compile and Extract Stack Maps

Compile to an object file:

```bash
llc -filetype=obj output.ll -o output.o
```

The object file contains a `.llvm_stackmaps` section with metadata about:
- Where each safepoint is (instruction offset)
- Which GC pointers are live
- Where those pointers are stored (registers, stack offsets)

### Parse Stack Maps

**See:** `rustlang/src/stackmap.rs` for a complete parser.

The stack map format is documented here: https://llvm.org/docs/StackMaps.html

**Key structure:**
```rust
struct StackMapRecord {
    id: u64,                    // Unique safepoint ID
    instruction_offset: u32,    // Where in the function
    locations: Vec<Location>,   // Where live pointers are
}

struct Location {
    type: LocationType,  // Register, Indirect (stack), etc.
    reg: u16,           // DWARF register number
    offset: i32,        // Offset for stack locations
}
```

## Step 5: Implement the GC Protocol

At runtime, when GC triggers:

1. **Suspend execution** at a safepoint
2. **Look up stack map** for current instruction pointer
3. **Read root pointers** from locations specified in stack map
4. **Perform GC** (mark, copy, whatever your algorithm does)
5. **Write back updated pointers** to same locations
6. **Resume execution**

### Example: Reading Roots from Stack Map

```rust
// At safepoint, instruction pointer is 0x1234
let record = find_stackmap_record(0x1234);

let mut roots = Vec::new();
for location in record.gc_locations() {
    match location.type {
        Indirect => {
            // Pointer is on stack
            let addr = stack_base + location.offset;
            let ptr = read_ptr(addr);
            roots.push((addr, ptr));
        }
        Register => {
            // Pointer is in register (saved to stack by LLVM)
            // ...
        }
    }
}

// Do GC
gc_collect(&mut roots);

// Write back relocated pointers
for (addr, new_ptr) in roots {
    write_ptr(addr, new_ptr);
}
```

**See:** `rustlang/src/main.rs:306-362` for a simplified simulation.

## Step 6: Handle Your Language's Types

### Object Layout

Every GC object needs:
- **Header** with size, type tag, forwarding pointer
- **Payload** with your data

```c
struct Object {
    size_t size;
    void* forwarding;    // Used during copying GC
    uint32_t type_tag;   // For polymorphism
    // ... your fields
};
```

### Scanning Object Interiors

The demo GC doesn't scan object contents (see `gc_runtime.rs:211-213`). You need to:

1. **Tag objects with type information**
2. **Implement scanning** based on type
3. **Recursively copy** objects pointed to by fields

```rust
fn scan_object(&mut self, obj: *mut u8) {
    let type_tag = get_type_tag(obj);

    match type_tag {
        TYPE_TUPLE => {
            // Scan each field
            for i in 0..tuple_length(obj) {
                let field_ptr = get_field(obj, i);
                let new_ptr = copy_object(field_ptr);
                set_field(obj, i, new_ptr);
            }
        }
        TYPE_CLOSURE => {
            // Scan captured variables
            // ...
        }
        TYPE_INT => {
            // No pointers, nothing to scan
        }
    }
}
```

## Step 7: Integrate with Your Compiler

### Using inkwell (Rust)

```rust
use inkwell::context::Context;
use inkwell::AddressSpace;

let context = Context::create();
let module = context.create_module("my_module");
let builder = context.create_builder();

// GC pointer type
let gc_ptr = context.ptr_type(AddressSpace::from(1));

// Declare gc_alloc
let i64_type = context.i64_type();
let alloc_type = gc_ptr.fn_type(&[i64_type.into()], false);
let gc_alloc = module.add_function("gc_alloc", alloc_type, Some(Linkage::External));

// Create function with GC
let fn_type = i64_type.fn_type(&[], false);
let function = module.add_function("my_func", fn_type, None);
function.set_gc("statepoint-example");  // ← Critical!

// Generate IR
let entry = context.append_basic_block(function, "entry");
builder.position_at_end(entry);

// Allocate
let size = i64_type.const_int(64, false);
let obj = builder.build_call(gc_alloc, &[size.into()], "obj")
    .unwrap()
    .try_as_basic_value()
    .left()
    .unwrap()
    .into_pointer_value();

// Use obj...
```

**See:** `rustlang/src/main.rs:172-212` for full example.

### Using LLVM C++ API

```cpp
LLVMContext context;
Module module("my_module", context);
IRBuilder<> builder(context);

// GC pointer type
Type* gc_ptr = PointerType::get(Type::getInt8Ty(context), 1);

// Function with GC
FunctionType* fn_type = FunctionType::get(
    Type::getInt64Ty(context), {}, false);
Function* fn = Function::Create(fn_type, Function::ExternalLinkage,
    "my_func", module);
fn->setGC("statepoint-example");

BasicBlock* entry = BasicBlock::Create(context, "entry", fn);
builder.SetInsertPoint(entry);

// ... generate IR
```

## Step 8: Link Everything Together

```bash
# Compile your IR to object file (with stack maps)
llc -filetype=obj my_program.ll -o my_program.o

# Compile GC runtime
rustc --crate-type=cdylib gc_runtime.rs -o libgc.so

# Link together
clang my_program.o -L. -lgc -o my_program

# Extract stack maps for runtime use
objcopy --dump-section .llvm_stackmaps=stackmaps.bin my_program.o
```

At runtime, load `stackmaps.bin` and use it to find roots during GC.

## Common Pitfalls

### 1. Forgetting addrspace(1)
```llvm
; WRONG - LLVM won't track this
%obj = call ptr @gc_alloc(i64 64)

; RIGHT
%obj = call ptr addrspace(1) @gc_alloc(i64 64)
```

### 2. Not Setting GC Strategy
```llvm
; WRONG - no statepoints generated
define i64 @func() {

; RIGHT
define i64 @func() gc "statepoint-example" {
```

### 3. Using Pointers After Safepoints
```llvm
; WRONG
%obj = call ptr addrspace(1) @gc_alloc(i64 64)
call void @might_trigger_gc()    ; obj might move!
store i64 42, ptr addrspace(1) %obj  ; using stale pointer!

; Let LLVM handle it - it will insert gc.relocate
```

### 4. Mixing GC and Non-GC Pointers
- Stack-allocated locals: regular pointers
- Heap-allocated objects: `addrspace(1)` pointers
- Never cast between them without proper barriers

## Concurrency Considerations

The demo uses a global GC state which isn't thread-safe. For concurrency:

### Option 1: Stop-the-World
- Suspend all threads at safepoints
- Walk each thread's stack using stack maps
- Collect roots from all threads
- Perform single-threaded GC
- Resume threads

### Option 2: Per-Thread Heaps
- Each thread has its own nursery
- Thread-local allocation (no locks)
- Global heap for shared objects
- More complex GC protocol

### Option 3: Concurrent GC
- Requires write barriers
- LLVM can help with this too (gc.statepoint supports barriers)
- Much more complex to implement

## Testing Your Implementation

### Start Simple
1. Allocate one object, trigger GC, verify it moved
2. Allocate two objects, keep one live, verify GC works
3. Test object graphs (objects pointing to objects)
4. Test many allocations forcing multiple GCs

### Verify Correctness
```rust
// Pattern to test GC moving objects
let obj = gc_alloc(64);
*(obj as *mut u64) = 0xDEADBEEF;

let original_addr = obj as usize;

// Trigger GC
gc_collect();

// obj should be updated to new address
let new_addr = obj as usize;
assert_ne!(original_addr, new_addr);
assert_eq!(*(obj as *const u64), 0xDEADBEEF);
```

**See:** `rustlang/src/main.rs:20-151` for a complete test.

## Next Steps

1. **Start with the demo** - Run it, understand the output
2. **Build a minimal language** - Just integers and function calls
3. **Add GC types** - Tuples/pairs that contain pointers
4. **Implement proper scanning** - Make GC trace object graphs
5. **Add your language features** - Pattern matching, closures, etc.
6. **Optimize** - Generational GC, write barriers, etc.

## Resources

- LLVM Statepoints: https://llvm.org/docs/Statepoints.html
- Stack Maps: https://llvm.org/docs/StackMaps.html
- GC Strategy: https://llvm.org/docs/GarbageCollection.html
- This demo: `rustlang/src/main.rs`, `gc_runtime.rs`, `stackmap.rs`

## Example: Minimal Language Pipeline

```rust
// 1. Parse source
let ast = parse("let x = 42 in x + 1");

// 2. Generate LLVM IR with GC annotations
let ir = codegen_with_gc(ast);

// 3. Run statepoint pass
run_opt_pass(ir, "rewrite-statepoints-for-gc");

// 4. Compile to object file
let obj = compile_to_object(ir);

// 5. Parse stack maps from object
let stackmaps = parse_stackmaps(obj);

// 6. Link with GC runtime
link_with_runtime(obj, gc_runtime, stackmaps);

// 7. Run!
execute();
```

The hard parts are #2 (correct IR generation) and implementing a robust GC runtime. Everything else is plumbing.

Good luck! Start small, test thoroughly, and incrementally add features.
