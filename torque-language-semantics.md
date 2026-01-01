# Understanding V8 Torque for JVM Bytecode Generation

This document explains how Torque works as a language and how its constructs map to JVM bytecode.

---

## 1. What Torque Actually Is

Torque is a **statically-typed imperative language** designed to express JavaScript builtin implementations. It sits at an interesting level of abstraction:

```
ECMAScript Spec (prose)
        ↓
    Torque (.tq)         ← You read this
        ↓
CodeStubAssembler (C++)  ← V8's IR
        ↓
    Machine Code
```

Torque's key insight: JavaScript builtins have **predictable patterns** that can be expressed more safely with static types while still allowing low-level control.

---

## 2. The Type System

### 2.1 Tagged Values

JavaScript values in V8 are "tagged" - a single machine word that's either:
- A **Smi** (Small Integer): Integer stored directly in the pointer with a tag bit
- A **HeapObject**: Pointer to heap-allocated data

```torque
// Torque represents this with a type hierarchy
type Object;                    // Any JS value
type Smi extends Object;        // Tagged small integer
type HeapObject extends Object; // Pointer to heap

type Number = Smi | HeapNumber; // Union type
```

### JVM Mapping

The JVM doesn't have tagged pointers, so you have choices:

**Option A: Boxed Everything**
```java
// Every value is a JSValue object
public abstract sealed class JSValue
    permits JSNumber, JSString, JSObject, JSUndefined, JSNull, JSBoolean {}

public final class JSNumber extends JSValue {
    private final double value;  // Always use double internally
}
```

**Option B: Primitive Specialization (faster)**
```java
// Use primitive where possible, box otherwise
public sealed interface JSValue {}

// For hot paths, generate specialized versions:
public int addSmiSmi(int a, int b) { return a + b; }
public double addNumbers(double a, double b) { return a + b; }
public JSValue addGeneric(JSValue a, JSValue b) { /* slow path */ }
```

### 2.2 Type Hierarchy

Torque's types form a lattice:

```
                    Object
                   /      \
                Smi    HeapObject
                         /    \
                  HeapNumber  JSReceiver
                              /        \
                        JSObject    JSProxy
                        /      \
                   JSArray  JSFunction
```

**JVM bytecode consideration**: The `instanceof` instruction is your friend. Generate type checks as:

```
; Check if value is JSArray
aload 1              ; load value
instanceof JSArray   ; push 1 if true, 0 if false
ifeq slow_path       ; branch if not JSArray
```

### 2.3 Union Types

Torque allows union types:

```torque
type Number = Smi | HeapNumber;
type Numeric = Number | BigInt;
```

**JVM Mapping**: Sealed interfaces with pattern matching (Java 21+):

```java
public sealed interface JSNumeric permits JSNumber, JSBigInt {}

// Usage with pattern matching
switch (value) {
    case JSNumber n -> handleNumber(n);
    case JSBigInt b -> handleBigInt(b);
}
```

Or for JVM bytecode directly, generate a type check ladder:

```
aload 1
instanceof JSNumber
ifne handle_number
aload 1
instanceof JSBigInt
ifne handle_bigint
; fall through to error
```

---

## 3. Callables: Macros vs Builtins

Torque has two main callable types with very different semantics:

### 3.1 Macros

Macros are **inlined at call sites** - they don't generate separate functions.

```torque
macro IsJSArray(o: Object): bool {
  return Is<JSArray>(o);
}

// When you call it:
if (IsJSArray(x)) { ... }

// It's as if you wrote:
if (Is<JSArray>(x)) { ... }
```

**JVM Mapping**: You have two choices:

1. **Actual inlining** - Copy the bytecode at each call site
2. **Private static methods** - Let the JIT inline them

```java
// Generate as a static method, rely on JIT inlining
private static boolean isJSArray(JSValue o) {
    return o instanceof JSArray;
}
```

The JVM's JIT is very good at inlining small methods, so option 2 is usually fine.

### 3.2 Builtins

Builtins are **real callable functions** exposed to JavaScript:

```torque
javascript builtin ArrayPush(
    context: Context,
    receiver: Object,
    ...arguments): Object {
  // implementation
}
```

The `javascript` keyword means this is callable from JS with standard calling convention.

**JVM Mapping**: Generate a class implementing a `JSBuiltin` interface:

```java
public class ArrayPush implements JSBuiltin {
    @Override
    public JSValue call(ExecutionContext ctx,
                        JSValue thisArg,
                        JSValue[] args) {
        // generated implementation
    }
}
```

**Bytecode for varargs**:
```
; Load arguments array
aload 3              ; args parameter
arraylength          ; get length
istore 4             ; store in local

; Iterate through arguments
iconst_0
istore 5             ; i = 0
loop:
  iload 5
  iload 4
  if_icmpge done     ; if i >= length, done
  aload 3
  iload 5
  aaload             ; args[i]
  ; ... process argument
  iinc 5 1           ; i++
  goto loop
done:
```

### 3.3 Runtime Functions

```torque
extern runtime ToString(context: Context, o: Object): String;
```

These call into the runtime system (C++ in V8). For your JVM implementation, these become calls to your runtime library:

```java
// Your runtime class
public class Runtime {
    public static JSString toString(ExecutionContext ctx, JSValue o) {
        // Implement per ECMAScript spec
    }
}
```

**Bytecode**:
```
aload 0              ; context
aload 1              ; object
invokestatic Runtime.toString(LExecutionContext;LJSValue;)LJSString;
```

---

## 4. Control Flow

### 4.1 Labels and Goto

This is the most interesting part. Torque uses **labels** for non-local control flow:

```torque
macro TryGetProperty(o: Object, key: String): Object
    labels NotFound {
  const result = GetProperty(o, key);
  if (result == Undefined) goto NotFound;
  return result;
}

// Usage:
try {
  const value = TryGetProperty(obj, "foo") otherwise NotFound;
  // use value
}
label NotFound {
  // handle not found
}
```

This is like exceptions, but more explicit and with the ability to pass values.

**JVM Mapping Options**:

**Option A: Exceptions (simplest)**

```java
public static class NotFoundLabel extends RuntimeException {
    // Use a singleton to avoid allocation
    public static final NotFoundLabel INSTANCE = new NotFoundLabel();
    private NotFoundLabel() { super(null, null, false, false); }
}

public static JSValue tryGetProperty(JSValue o, JSString key)
    throws NotFoundLabel {
    JSValue result = getProperty(o, key);
    if (result == JSUndefined.INSTANCE) {
        throw NotFoundLabel.INSTANCE;
    }
    return result;
}
```

**Bytecode**:
```
; try block
invokestatic tryGetProperty
goto after_catch
; catch block
catch NotFoundLabel
  ; handle not found
after_catch:
```

**Option B: Return Unions (no exceptions)**

```java
public sealed interface GetPropertyResult {
    record Found(JSValue value) implements GetPropertyResult {}
    record NotFound() implements GetPropertyResult {}
}

public static GetPropertyResult tryGetProperty(JSValue o, JSString key) {
    JSValue result = getProperty(o, key);
    if (result == JSUndefined.INSTANCE) {
        return new GetPropertyResult.NotFound();
    }
    return new GetPropertyResult.Found(result);
}
```

**Option C: Continuation Passing (advanced)**

For complex control flow, you could use CPS transformation:

```java
public interface NotFoundContinuation {
    JSValue run();
}

public static JSValue tryGetProperty(
    JSValue o, JSString key,
    NotFoundContinuation onNotFound) {
    JSValue result = getProperty(o, key);
    if (result == JSUndefined.INSTANCE) {
        return onNotFound.run();
    }
    return result;
}
```

### 4.2 Typeswitch

Torque's `typeswitch` is a type-narrowing control structure:

```torque
typeswitch (value) {
  case (s: Smi): {
    // Here, s is known to be Smi
    return s + 1;
  }
  case (n: HeapNumber): {
    // Here, n is known to be HeapNumber
    return n + 1.0;
  }
  case (o: Object): {
    // Catch-all
    return Undefined;
  }
}
```

**Key semantics**:
- Cases are checked **in order**
- First matching case wins
- The variable is **rebound** with the narrowed type
- Must be exhaustive (cover all possibilities)

**JVM Bytecode**:

```
aload 1                    ; load value
instanceof Smi             ; is it Smi?
ifeq not_smi
; --- Smi case ---
aload 1
checkcast Smi              ; narrow type (for verifier)
invokevirtual Smi.intValue
iconst_1
iadd
invokestatic JSNumber.fromInt
areturn

not_smi:
aload 1
instanceof HeapNumber
ifeq not_heapnumber
; --- HeapNumber case ---
aload 1
checkcast HeapNumber
invokevirtual HeapNumber.doubleValue
dconst_1
dadd
invokestatic JSNumber.fromDouble
areturn

not_heapnumber:
; --- Object case (catch-all) ---
getstatic JSUndefined.INSTANCE
areturn
```

With Java 21+ pattern matching:
```java
return switch (value) {
    case Smi s -> JSNumber.fromInt(s.intValue() + 1);
    case HeapNumber n -> JSNumber.fromDouble(n.doubleValue() + 1.0);
    default -> JSUndefined.INSTANCE;
};
```

### 4.3 Try/Label Blocks

```torque
try {
  const result = SomeOperation() otherwise Bailout;
  return result;
}
label Bailout {
  return SlowPath();
}
```

This is structured exception handling with explicit labels.

**JVM Mapping**:
```
try_start:
  invokestatic SomeOperation    ; may throw BailoutLabel
  areturn                       ; return result
try_end:
  goto after_handler
handler:
  pop                           ; discard exception
  invokestatic SlowPath
  areturn
after_handler:

; Exception table entry:
; from=try_start, to=try_end, target=handler, type=BailoutLabel
```

---

## 5. Memory Operations

### 5.1 Object Field Access

```torque
// Reading a field
const length = array.length;

// Writing a field
array.length = newLength;
```

**JVM Mapping**: Depends on your object representation.

**Option A: Real fields**
```java
public class JSArray extends JSObject {
    public int length;  // Direct field
}
```

```
aload 1              ; array
getfield JSArray.length I
```

**Option B: Property maps (more flexible)**
```java
public class JSObject {
    private Object[] properties;
    private HiddenClass hiddenClass;

    public Object getProperty(int slot) {
        return properties[slot];
    }
}
```

```
aload 1              ; array
iconst_0             ; slot for 'length'
invokevirtual JSObject.getProperty(I)Ljava/lang/Object;
```

### 5.2 Array Element Access

```torque
const element = array.elements[index];
```

**JVM bytecode**:
```
aload 1              ; array
invokevirtual JSArray.getElements()[LJSValue;
iload 2              ; index
aaload               ; elements[index]
```

With bounds checking:
```
aload 1              ; array
iload 2              ; index
invokevirtual JSArray.getElement(I)LJSValue;  ; does bounds check internally
```

---

## 6. Intrinsics

Torque has intrinsic functions prefixed with `%`:

```torque
%RawDownCast<JSArray>(object)  // Unsafe cast without check
%GetMap(object)                 // Get hidden class/map
%SmiTag(intValue)              // Convert int to Smi
%SmiUntag(smi)                 // Convert Smi to int
```

**JVM Mappings**:

| Intrinsic | JVM Equivalent |
|-----------|----------------|
| `%RawDownCast<T>` | `checkcast T` (or just trust it) |
| `%GetMap` | `invokevirtual getHiddenClass` |
| `%SmiTag` | No-op if using int directly |
| `%SmiUntag` | No-op if using int directly |
| `%BranchIfSmi` | `instanceof` check |
| `%Word32Equal` | `if_icmpeq` |

---

## 7. Code Generation Example

Let's trace through a complete example:

### Torque Input

```torque
javascript builtin MathMax(
    context: Context,
    receiver: Object,
    ...arguments): Number {
  let max: Number = NegativeInfinity;

  for (let i: intptr = 0; i < arguments.length; i++) {
    const arg = arguments[i];
    const num: Number = ToNumber(context, arg);

    typeswitch (num) {
      case (s: Smi): {
        typeswitch (max) {
          case (maxSmi: Smi): {
            if (s > maxSmi) max = s;
          }
          case (HeapNumber): {
            if (Convert<float64>(s) > max) max = s;
          }
        }
      }
      case (n: HeapNumber): {
        if (n > max || NumberIsNaN(n)) max = n;
      }
    }
  }

  return max;
}
```

### Generated Java

```java
public class MathMax implements JSBuiltin {
    @Override
    public JSValue call(ExecutionContext context,
                        JSValue receiver,
                        JSValue[] arguments) {
        JSNumber max = JSNumber.NEGATIVE_INFINITY;

        for (int i = 0; i < arguments.length; i++) {
            JSValue arg = arguments[i];
            JSNumber num = Runtime.toNumber(context, arg);

            if (num.isSmi()) {
                int s = num.smiValue();
                if (max.isSmi()) {
                    int maxSmi = max.smiValue();
                    if (s > maxSmi) max = num;
                } else {
                    if ((double)s > max.doubleValue()) max = num;
                }
            } else {
                double n = num.doubleValue();
                if (n > max.doubleValue() || Double.isNaN(n)) {
                    max = num;
                }
            }
        }

        return max;
    }
}
```

### JVM Bytecode (simplified)

```
; Prologue
getstatic JSNumber.NEGATIVE_INFINITY  ; max = -Infinity
astore 4                              ; local 4 = max
iconst_0
istore 5                              ; local 5 = i

loop:
  iload 5                             ; i
  aload 3                             ; arguments
  arraylength
  if_icmpge done                      ; if i >= length, done

  ; arg = arguments[i]
  aload 3
  iload 5
  aaload
  astore 6                            ; local 6 = arg

  ; num = ToNumber(context, arg)
  aload 1                             ; context
  aload 6                             ; arg
  invokestatic Runtime.toNumber
  astore 7                            ; local 7 = num

  ; typeswitch on num
  aload 7
  invokevirtual JSNumber.isSmi
  ifeq not_smi

  ; Smi case
  aload 7
  invokevirtual JSNumber.smiValue
  istore 8                            ; local 8 = s

  ; nested typeswitch on max
  aload 4
  invokevirtual JSNumber.isSmi
  ifeq max_is_heap

  ; max is also Smi
  aload 4
  invokevirtual JSNumber.smiValue
  istore 9                            ; local 9 = maxSmi
  iload 8                             ; s
  iload 9                             ; maxSmi
  if_icmple next_iter                 ; if s <= maxSmi, skip
  aload 7
  astore 4                            ; max = num
  goto next_iter

max_is_heap:
  iload 8                             ; s
  i2d                                 ; convert to double
  aload 4
  invokevirtual JSNumber.doubleValue
  dcmpg
  ifle next_iter                      ; if s <= max, skip
  aload 7
  astore 4                            ; max = num
  goto next_iter

not_smi:
  ; HeapNumber case
  aload 7
  invokevirtual JSNumber.doubleValue
  dstore 8                            ; local 8-9 = n (double)

  ; if (n > max || isNaN(n))
  dload 8
  aload 4
  invokevirtual JSNumber.doubleValue
  dcmpg
  ifgt update_max
  dload 8
  invokestatic Double.isNaN
  ifeq next_iter

update_max:
  aload 7
  astore 4                            ; max = num

next_iter:
  iinc 5 1                            ; i++
  goto loop

done:
  aload 4                             ; return max
  areturn
```

---

## 8. Key Semantic Patterns

### 8.1 ToNumber, ToString, etc.

These abstract operations from the spec appear everywhere:

```torque
const num: Number = ToNumber(context, arg);
const str: String = ToString(context, arg);
```

You'll need runtime implementations per ECMAScript spec.

### 8.2 Fast Path / Slow Path

Torque uses labels for fast/slow path branching:

```torque
macro FastArrayPush(array: JSArray, value: Object): Smi
    labels Slow {
  if (array.length >= array.elements.length) goto Slow;
  array.elements[array.length] = value;
  return array.length++;
}

// Usage:
try {
  return FastArrayPush(array, value) otherwise Slow;
} label Slow {
  return GenericArrayPush(array, value);
}
```

For JVM: Use exception-based control flow or result types.

### 8.3 Assertions and Dchecks

```torque
dcheck(x != 0);  // Debug-only assertion
check(x != 0);   // Always checked
```

**JVM Mapping**:
```java
assert x != 0;  // For dcheck (disabled in production)
if (x == 0) throw new TypeError("...");  // For check
```

---

## 9. What You Need to Implement

### Core Runtime

| Component | Complexity |
|-----------|------------|
| Type hierarchy (JSValue, etc.) | Low |
| Property access | Medium |
| ToNumber/ToString/ToBoolean | Medium |
| Object creation | Medium |
| Array operations | Medium |
| Exception handling | Medium |
| Hidden classes (optional) | High |

### Builtins by Priority

1. **Object** - Object.keys, Object.prototype.hasOwnProperty, etc.
2. **Array** - push, pop, map, filter, reduce, forEach
3. **String** - charAt, substring, indexOf, etc.
4. **Number** - toString, isNaN, isFinite
5. **Math** - abs, floor, ceil, max, min, random
6. **Function** - call, apply, bind

### What to Skip Initially

- Proxy/Reflect (complex)
- WeakMap/WeakSet (requires weak references)
- SharedArrayBuffer (concurrency)
- Intl (massive surface area)
