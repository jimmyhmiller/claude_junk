# Node.js/V8 Runtime Specification Analysis

## Investigation Summary

This document analyzes Node.js/V8's runtime specification languages and assesses their applicability to a Java-based JavaScript interpreter.

---

## 1. What Node.js/V8 Uses

V8 (and by extension Node.js) uses **multiple specification layers** to define JavaScript runtime objects:

### Primary: V8 Torque Language

**Torque** is V8's domain-specific language for specifying and implementing JavaScript builtins.

| Aspect | Details |
|--------|---------|
| **File Extension** | `.tq` |
| **Location** | `v8/src/builtins/*.tq` and `v8/src/objects/*.tq` |
| **Lines of Code** | ~20,000+ lines |
| **Purpose** | Define JavaScript engine builtins with strong typing |

#### Example Torque Syntax (from math.tq):

```torque
namespace math {
  javascript builtin MathIs42(
      context: Context, receiver: Object, x: Object): Boolean {
    const number: Number = ToNumber_Inline(x);
    typeswitch (number) {
      case (smi: Smi): {
        return smi == 42 ? True : False;
      }
      case (heapNumber: HeapNumber): {
        return IsNumberEqual(IsEqual(heapNumber, 42.0));
      }
    }
  }
}
```

#### Key Torque Features:
- **Static typing** with V8-specific types (`Smi`, `HeapNumber`, `JSArray`, etc.)
- **TypeScript-like syntax** for readability
- **Compiles to CodeStubAssembler** (CSA), then to machine code
- **ECMAScript spec alignment** - designed to directly translate spec prose
- **Low-level optimization primitives** for performance-critical code

### Secondary Layers

| Layer | Format | Purpose |
|-------|--------|---------|
| **C++ Runtime Functions** | `runtime.h` macros | Internal runtime with `%FunctionName()` syntax |
| **JavaScript Bootstrap** | `lib/internal/bootstrap/*.js` | Node.js-specific initialization |
| **Primordials** | JavaScript + C++ | Frozen copies of intrinsics (pollution-resistant) |
| **WebIDL** (Blink integration) | `.idl` files | DOM/Web API bindings |

---

## 2. Completeness Assessment

### Coverage by Category

| Category | Completeness | Notes |
|----------|--------------|-------|
| **ECMAScript Builtins** | ✅ Complete | Array, Object, String, Math, Promise, etc. |
| **Type System** | ✅ Complete | All JS types + V8 internal representations |
| **Object Shapes** | ✅ Complete | Hidden classes, property descriptors |
| **Method Signatures** | ✅ Complete | Parameters, return types, optional args |
| **Optimization Hints** | ✅ Complete | Fast-path conditions, inline caching |
| **Error Handling** | ✅ Complete | Exception types, stack traces |
| **Memory Layout** | ✅ Complete | GC heap object structures |

### What Torque Covers Well:
- All standard ECMAScript built-in objects
- Complete method implementations (not just signatures)
- Type coercion rules and edge cases
- Performance-critical optimizations
- Internal V8 object representation

### What Torque Does NOT Provide:
- **Declarative specification** - It's imperative implementation code
- **Platform independence** - Tightly coupled to V8 internals
- **Semantic specification** - No formal behavioral contracts
- **Test specifications** - Tests are separate (in Test262)

---

## 3. Feasibility for Java-Based Interpreter

### TL;DR: **Difficult but possible with significant effort**

### Challenges

#### 1. V8-Specific Coupling (HIGH)
```
Torque types → V8 C++ types → V8 memory model
```
- `Smi`, `HeapNumber`, `Map`, `TNode<T>` are V8-specific
- Object layout assumes V8's hidden class system
- No abstraction layer for alternative backends

#### 2. Compilation Target (HIGH)
- Torque → CodeStubAssembler → Machine code
- No IR suitable for JVM bytecode emission
- Would need to write a completely new Torque backend

#### 3. Imperative vs Declarative (MEDIUM)
- Torque contains **implementation**, not just **specification**
- Many V8-specific optimizations mixed with semantics
- Hard to extract "what" from "how"

### Alternative Approaches for Java

#### Option A: Parse Torque, Extract Semantics
| Effort | Risk | Benefit |
|--------|------|---------|
| Very High | High | Full fidelity to V8 behavior |

- Write a Torque parser
- Build semantic analyzer to extract method signatures/behaviors
- Generate Java implementation stubs
- Manually implement each builtin

#### Option B: Use TypeScript lib.d.ts Definitions
| Effort | Risk | Benefit |
|--------|------|---------|
| Medium | Low | Good type coverage |

```typescript
// From lib.es2015.core.d.ts
interface Array<T> {
    find<S extends T>(predicate: (value: T, index: number, obj: T[]) => value is S): S | undefined;
    findIndex(predicate: (value: T, index: number, obj: T[]) => unknown): number;
    fill(value: T, start?: number, end?: number): this;
    copyWithin(target: number, start: number, end?: number): this;
}
```

**Pros:**
- Already a declarative format
- Complete coverage of standard APIs
- Well-maintained by Microsoft
- Easy to parse (TypeScript AST)

**Cons:**
- Types only, no implementation
- No internal semantics

#### Option C: Use ECMAScript Spec + Test262
| Effort | Risk | Benefit |
|--------|------|---------|
| High | Low | Authoritative source |

- ECMAScript spec is the authoritative behavioral specification
- Test262 provides conformance tests
- Multiple Java engines already do this (GraalJS, Rhino)

#### Option D: Leverage Existing Java Engines

| Engine | Status | Approach |
|--------|--------|----------|
| **GraalJS** | Active | Uses Truffle framework, ECMAScript 2024 compliant |
| **Nashorn** | Deprecated (JDK 15) | Direct spec implementation |
| **Rhino** | Maintained | Community-driven, older spec compliance |

GraalJS source shows how they implement builtins:
```java
// GraalJS approach - direct Java implementation
@TruffleBoundary
public static Object arrayFrom(Object thisObj, Object arrayLike, Object mapFn, Object thisArg) {
    // Implementation follows ECMAScript spec directly
}
```

---

## 4. Recommendations

### For a New Java Interpreter

1. **Don't use Torque directly** - Too coupled to V8
2. **Use TypeScript definitions** for type signatures
3. **Follow ECMAScript spec** for implementation semantics
4. **Use Test262** for conformance validation
5. **Study GraalJS source** for implementation patterns

### Toolchain Suggestion

```
TypeScript lib.d.ts → Parse → Generate Java interfaces
                             ↓
ECMAScript Spec → Manual implementation → Java classes
                             ↓
Test262 → Conformance testing → Validation
```

### Estimated Effort

| Component | Lines of Code | Effort |
|-----------|---------------|--------|
| Core Objects (Object, Array, String) | ~10,000 | 3-6 months |
| Standard Library (Math, Date, RegExp) | ~8,000 | 2-4 months |
| ES6+ Features (Promise, Proxy, Symbol) | ~12,000 | 4-6 months |
| Full ES2024 Compliance | ~40,000 | 12-18 months |

---

## 5. Related Resources

### Specification Sources
- [ECMAScript Specification](https://tc39.es/ecma262/)
- [Test262 Conformance Suite](https://github.com/tc39/test262)
- [TypeScript lib.d.ts](https://github.com/microsoft/TypeScript/tree/main/src/lib)

### V8/Torque Documentation
- [V8 Torque User Manual](https://v8.dev/docs/torque)
- [V8 Torque Builtins Guide](https://v8.dev/docs/torque-builtins)
- [V8 src/builtins](https://github.com/v8/v8/tree/main/src/builtins)
- [V8 math.tq](https://github.com/v8/v8/blob/main/src/builtins/math.tq)
- [V8 array.tq](https://github.com/v8/v8/blob/master/src/builtins/array.tq)

### Java Engines
- [GraalJS](https://www.graalvm.org/latest/reference-manual/js/)
- [GraalJS Source](https://github.com/oracle/graaljs)
- [Rhino](https://github.com/mozilla/rhino)

### WinterCG Cross-Runtime
- [Runtime Keys Proposal](https://runtime-keys.proposal.wintercg.org/)
- [runtime-compat-data](https://www.npmjs.com/package/runtime-compat-data)

---

## Conclusion

V8's Torque language is **complete** for its purpose (implementing V8 builtins with high performance) but is **not suitable for direct use** in a Java-based interpreter due to:

1. Deep coupling to V8's C++ object model
2. Imperative implementation rather than declarative specification
3. No bytecode/IR generation for non-V8 targets

**Recommended path**: Use TypeScript type definitions + ECMAScript spec + Test262, following the approach used by GraalJS.
