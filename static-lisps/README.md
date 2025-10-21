# Obscure Statically Typed Lisps

A collection of weird, interesting, and academically significant statically typed Lisp dialects. These are languages typed to their core, not optional type systems like Typed Racket or Clojure's core.typed.

## Table of Contents

1. [Linear Lisp (Baker, 1992)](#linear-lisp)
2. [FX-87 (1988)](#fx-87)
3. [Pre-Scheme](#pre-scheme)
4. [Alms](#alms)
5. [Interim](#interim)
6. [MOLL (My Own Linear Lisp)](#moll)
7. [Lux](#lux)
8. [Coalton](#coalton)
9. [BLisp](#blisp)
10. [Indigo Lisp](#indigo-lisp)

---

## Linear Lisp

**Link:** [Lively Linear Lisp — 'Look Ma, No Garbage!' (PDF)](https://www.cs.utexas.edu/~hunt/research/hash-cons/hash-cons-papers/BakerLinearLisp.pdf)

**Author:** Henry G. Baker
**Published:** ACM SIGPLAN Notices 27, 8 (Aug. 1992), 89-98

### Description

Linear Lisp is a Lisp-like language based on linear logic that eliminates the need for garbage collection entirely. In Linear Lisp, every value must be used exactly once - no more, no less. This corresponds to the principles of linear logic where weakening and contraction are disallowed.

### Type System

- **Linear types**: Every variable must be used exactly once
- List cell reference counts are conserved and always identically 1
- Correspondence with Linear Lambda Calculus and Linear Logic

### Memory Management

**No garbage collection needed!** The language guarantees that storage is managed as safely as non-linear Lisp but runs within a constant factor of non-linear logic performance. Reference counts are implicit in the linear type system.

### Key Concepts

From the paper: Linear logic has been proposed as one solution to the problem of garbage collection and providing efficient "update-in-place" capabilities within a more functional language.

### Code Example

Traditional Lisp operations like `car` and `cdr` must be modified in Linear Lisp since they would violate linearity (using a cons cell multiple times). Instead, Linear Lisp uses operations like:

```lisp
;; deconstructing a pair uses it exactly once
(linear-let ((x y) pair)
  ... use x once and y once ...)
```

### Academic References

- Baker, H. G. (1992). "Lively Linear Lisp — 'Look Ma, No Garbage!'"
- Henry Baker's Archive: http://home.pipeline.com/~hbaker1/

---

## FX-87

**Link:** [Wikipedia](https://en.wikipedia.org/wiki/FX-87)

**Authors:** J.M. Lucassen et al.
**Published:** POPL 1988

### Description

FX-87 is a polymorphic typed functional language based on a system for static program analysis where every expression has two static properties: a **type** and an **effect**. This pioneered the concept of effect systems in statically-typed functional programming.

### Type System

- Polymorphic type system
- **Effect system**: tracks side effects at the type level
- Every expression has both a type and an effect
- KFX is the kernel language of FX-87

### Memory Management

Not specified in available sources, but likely automatic (GC-based) given the era and functional nature.

### Performance

FX-87 yields similar performance to other functional languages on pure programs (Fibonacci, Factorial), and showed great performance increases when matching DNA sequences.

### Academic References

- "Polymorphic Effect Systems", J.M. Lucassen et al., Proceedings of the 15th Annual ACM Conference POPL, ACM 1988, pp. 47–57

### Historical Significance

This is one of the earliest languages to incorporate an effect system for tracking computational effects at the type level, influencing modern languages like Koka, Eff, and others.

---

## Pre-Scheme

**Link:** [Pre-Scheme Homepage](https://prescheme.org/) | [Scheme48](https://s48.org/)

**Authors:** Richard Kelsey and Jonathan Rees (1986)
**Status:** Being restored via NGI Zero grant from NLnet Foundation

### Description

Pre-Scheme is a statically typed dialect of Scheme that combines the flexibility of Scheme with the efficiency and low-level machine access of C. Originally developed as the implementation language for the Scheme 48 virtual machine.

### Type System

- **Hindley-Milner type inference** (modified algorithm)
- Static typing with no additional runtime overhead
- Polymorphism via procedure copying
- Models Scheme's dynamic typing as accurately as possible at compile time

### Memory Management

Pre-Scheme compiles to C, giving you control over memory at a low level while maintaining safety. No garbage collection in the compiled output - suitable for systems programming, operating systems, and embedded systems.

### Code Example

Pre-Scheme looks like Scheme but with type restrictions:

```scheme
;; Pre-Scheme can be compiled to efficient C
(define (vector-length v)
  (- (header-length-in-bytes (header v))
     (header-length-in-bytes (header (make-vector 0)))))
```

### Use Cases

- Virtual machine implementation (Scheme 48 VM)
- Operating systems
- Embedded systems
- Anywhere C would be used but you want Lisp syntax

### Academic References

- "The Scheme 48 Implementation" - Richard Kelsey and Jonathan Rees
- FOSDEM 2023 talk: "Introduction to Pre-Scheme"

---

## Alms

**Link:** [GitHub](https://github.com/tov/alms) | [Project Page](https://users.cs.northwestern.edu/~jesse/pubs/alms/)

**Author:** Jesse Tov (Northwestern University)
**Published:** POPL 2011

### Description

Alms is a general-purpose programming language that supports **practical affine types**. It offers the expressiveness of Girard's linear logic while keeping the type system light and convenient.

### Type System

- **Affine types**: resources can be used *at most once* (vs linear types which must be used *exactly once*)
- Expressive kinds that minimize notation while maximizing polymorphism
- ML-style signature ascription for abstract affine types
- Subtyping support

### Memory Management

Manual memory management made safe via the affine type system. Resources are guaranteed not to be used more than once, preventing use-after-free and double-free errors.

### Key Features

An interface can impose stiffer resource usage restrictions than the principal usage restrictions of its implementation. This allows the type system to naturally express various resource management protocols from special-purpose type systems.

### Code Example

```ocaml
(* Affine types ensure file handles are used at most once *)
let read_file (f : File) : String * () =
  let contents = read_all f in
  let ()       = close f in  (* f cannot be used again after close *)
  (contents, ())
```

### Status

**No longer maintained** - doesn't build with latest GHC. Use Docker image `jessetov/alms` to try it out.

### Academic References

- "Practical Affine Types", Jesse A. Tov and Riccardo Pucella, POPL 2011
- ACM Digital Library: https://dl.acm.org/doi/10.1145/1926385.1926436

---

## Interim

**Link:** [GitHub](https://github.com/eudoxia0/interim)

**Author:** Fernando Borretti (eudoxia0)

### Description

Interim is a statically-typed, low-level dialect of Lisp featuring compile-time, **GC-free memory management** using regions. It's a technology demonstrator for region-based memory management.

### Type System

- Statically typed
- **Region-based memory management** at the type level
- No higher-order functions or higher-order types (intentionally simplified to focus on regions)

### Memory Management

**Region-based** - memory is organized into regions that can be allocated and deallocated as a unit. Compile-time analysis ensures safety without garbage collection.

Inspired by:
- Cyclone's region-based memory management
- Safe manual memory management research

### Code Example

```scheme
;; Regions are explicit in the type system
(region r
  (let ((x (allocate r 42)))
    (use x)))
;; x is deallocated when region r ends
```

### Limitations

Being a technology demonstrator, Interim lacks:
- Modules
- Macros
- Higher-order functions
- Higher-order types

### Build Requirements

- MLton (Standard ML compiler)
- git
- make

### Related Projects

- **Corvus**: Another low-level Lisp by the same author targeting LLVM

---

## MOLL (My Own Linear Lisp)

**Link:** [GitHub](https://github.com/fare/moll)

**Author:** Fare (François-René Rideau)

### Description

MOLL is an experimental implementation of a Linear Lisp, creating a maru-style evaluator. It's a low-key experiment exploring linear types in a Lisp context, inspired by ATS and Henry Baker's Linear Lisp work.

### Type System

- **Linear types**: Each value must be used exactly once
- Inspired by ATS (which has both dependent types and linear types)

### Memory Management

Linear type system eliminates need for garbage collection by ensuring every value is consumed exactly once.

### Status

Experimental - a technology exploration project.

### Inspiration

- Henry Baker's Linear Lisp paper
- ATS language's linear types
- Linear Logic principles

---

## Lux

**Link:** [Homepage](https://luxlang.github.io/lux/) | [GitHub](https://github.com/LuxLang/lux)

**Inspirations:** Clojure (syntax) + Haskell (functional programming) + Standard ML (polymorphism)

### Description

Lux is a functional, statically-typed Lisp that runs on multiple platforms (JVM, JavaScript, Python, Lua, Ruby). It's in beta stage with a stable JVM compiler and standard library.

### Type System

- **Hindley-Milner-style polymorphism**
- **Signatures and Structures** (inspired by ML's module system)
- Structural typing via the module system
- Unlike Haskell's type classes, structures are first-class values
- Dependent types support (macros can reason about types)

### Memory Management

Depends on host platform:
- JVM: Garbage collected
- JavaScript: Host GC
- Python/Lua/Ruby: Host memory management

### Code Examples

**Signature (like Haskell's type class):**

```clojure
(sig: #export (Ord a)
  (: (Eq a)
     eq)
  (: (-> a a Bool)
     <)
  (: (-> a a Bool)
     <=)
  (: (-> a a Bool)
     >)
  (: (-> a a Bool)
     >=))
```

**Structure (implementation):**

```clojure
(struct: #export Ord<Real>
  (ord;Ord Real)
  (def: eq Eq<Real>)
  (def: < r.<)
  (def: <= r.<=)
  (def: > r.>)
  (def: >= r.>=))
```

**Monoid for List:**

```clojure
(struct: #export Monoid<List>
  (All [a]
    (Monoid (List a)))
  (def: unit #;Nil)
  (def: (append xs ys)
    (case xs
      #;Nil
      ys

      (#;Cons x xs')
      (#;Cons x (append xs' ys)))))
```

**Higher-order function taking structure:**

```clojure
(def: #export (mapM monad f xs)
  (All [M a b]
    (-> (Monad M) (-> a (M b)) (List a) (M (List b))))
  (case xs
    #;Nil
    (:: monad wrap #;Nil)

    (#;Cons x xs')
    (do monad
      [_x (f x)
       _xs (mapM monad f xs')]
      (wrap (#;Cons _x _xs)))))
```

### Key Features

- Monadic macros (unlike most Lisps)
- Curried functions with partial application
- Multiple concurrency models: threads, async, FRP, STM, actor model
- Functions are curried by default

### Advantages over Haskell's Type Classes

By using ML-style modules instead of type classes:
1. Can have multiple implementations for the same type (no newtype hacks needed)
2. Structures are first-class runtime values
3. Can write functions that take and return structures
4. Can parameterize structures at runtime

---

## Coalton

**Link:** [Homepage](https://coalton-lang.github.io/) | [GitHub](https://github.com/coalton-lang/coalton)

**Authors:** Originally Robert Smith (2018), renovated by Elias Lawson-Fox and Cole Scott (2021)

### Description

Coalton adds tried-and-true **Hindley-Milner type checking** to Common Lisp. It's a language embedded inside of Lisp, allowing gradual adoption. Think of it as Standard ML/OCaml/Haskell embedded in Common Lisp.

### Type System

- **Hindley-Milner type inference**
- Strict, Static, and Strong typing
- Type classes (like Haskell)
- Parameterized algebraic data types (including mutually recursive types)
- Type defaulting system similar to Haskell

### Memory Management

Inherits Common Lisp's garbage collection - seamless interop with rest of the Lisp environment.

### Code Examples

**Algebraic Data Types:**

```lisp
(coalton-toplevel
  (define-type (Expr :t)
    (EInt Integer)
    (EAdd (Expr :t) (Expr :t))
    (EMul (Expr :t) (Expr :t))))
```

**Type Classes:**

```lisp
(coalton-toplevel
  (define-class (Eq :a)
    (== (:a -> :a -> Boolean))
    (/= (:a -> :a -> Boolean)))

  (define-instance (Eq Integer)
    (define == integer-equal?)
    (define /= integer-not-equal?)))
```

**Pattern Matching:**

```lisp
(coalton-toplevel
  (define (eval expr)
    (match expr
      ((EInt n) n)
      ((EAdd x y) (+ (eval x) (eval y)))
      ((EMul x y) (* (eval x) (eval y))))))
```

### Key Features

- Small, easy-to-understand language
- Advanced type checking techniques
- Doesn't need wholesale adoption (gradual typing)
- Easy interop with Common Lisp
- Used in commercial applications

### Examples Directory

The repo includes sophisticated examples:
- Symbolic differentiation
- Typing Haskell in Haskell (in Coalton!)
- Fibonacci via function exponentiation

### Commercial Usage

In development for ~5 years and currently used commercially.

---

## BLisp

**Link:** [Homepage](https://ytakano.github.io/blisp/) | [GitHub](https://github.com/ytakano/blisp)

**Author:** Yuuki Takano

### Description

BLisp is a statically typed Lisp-like programming language which adopts an **effect system** for `no_std` environments. Designed for embedded systems and scripting in Rust.

### Type System

- **Effect system**: distinguishes Pure from IO functions at the type level
- Algebraic data types with exhaustive pattern matching
- Type checking ensures incomplete pattern matches are rejected at compile time
- Higher-order functions with effect tracking

### Memory Management

Designed for `no_std` (no standard library) environments - **no garbage collection**. Memory management depends on the Rust host environment.

### Effect System Details

- **Pure**: Functions with no side effects. Cannot call IO functions.
- **IO**: Functions that can perform I/O operations

This distinction is enforced at compile time.

### Code Examples

**Pure Function (Factorial):**

```lisp
(export factorial (n)
  (Pure (-> (Int) Int))
  (if (<= n 0)
      1
      (* n (factorial (- n 1)))))
```

**Map Function:**

```lisp
(export map (f x)
  (Pure (-> ((Pure (-> (a) b)) '(a)) '(b)))
  ;; implementation
  )
```

**Pattern Matching with car:**

```lisp
(export car (x)
  (Pure (-> ('(t)) (Option t)))
  (match x
    ((Cons n _) (Some n))
    (_ None)))
```

**Type Signatures:**

- `(Pure (-> (Int Int) Bool))` - Pure function: Int, Int → Bool
- `(IO (-> (Int) []))` - IO function: Int → Unit

### Key Features

- Effect system prevents calling IO functions from pure contexts
- Exhaustive pattern matching
- Higher-order RPC support
- Integration with Rust

### Related Projects

- **blisp-repl**: REPL for BLisp
- **baremetalisp**: Toy OS project using similar concepts

---

## Indigo Lisp

**Link:** [GitHub](https://github.com/olewhalehunter/indigo-lisp)

**Author:** olewhalehunter

### Description

Algebraic Data Types, Pattern Matching Specifications, and Strong **Hindley-Milner Typing/Static Analysis Extensions** of Common Lisp.

### Type System

- **Hindley-Milner type system**
- Algebraic data types
- Pattern matching specifications
- Strong static analysis

### Memory Management

Inherits Common Lisp's garbage collection.

### Status

Appears to be a research/experimental project extending Common Lisp with modern type system features.

### Related Libraries

The Common Lisp ecosystem has several complementary libraries:

- **cl-algebraic-data-type** (by stylewarning): ADTs in the spirit of Haskell/Standard ML
- **Abacus**: Unified syntax for pattern matching over algebraic types
- **Trivia**: Community-standard pattern matching library

### Key Distinction

Unlike Coalton (which is a separate embedded language), Indigo Lisp appears to extend Common Lisp itself with type system features.

---

## Comparison Matrix

| Language | Year | Type System | Memory Mgmt | Linearity | Effects | Status |
|----------|------|-------------|-------------|-----------|---------|--------|
| FX-87 | 1988 | Polymorphic | GC | No | Yes | Historic |
| Linear Lisp | 1992 | Linear | None (linear) | Yes | No | Paper |
| Pre-Scheme | 1986+ | HM | Manual/C | No | No | Reviving |
| Alms | 2011 | Affine | Manual | Affine | No | Unmaintained |
| Interim | ~2015 | Static | Regions | No | No | Demo |
| MOLL | ~2015+ | Linear | None (linear) | Yes | No | Experimental |
| Lux | 2015+ | HM + Dependent | Host GC | No | No | Beta |
| Coalton | 2018+ | HM | GC | No | No | Production |
| BLisp | ~2020+ | Static | No GC | No | Yes | Active |
| Indigo | ~2020+ | HM | GC | No | No | Experimental |

## Key Insights

### Memory Management Approaches

1. **Linear/Affine Types**: Linear Lisp, MOLL, Alms
   - Eliminates GC via usage restrictions

2. **Region-based**: Interim
   - Allocate/deallocate regions as units

3. **Compile to C/native**: Pre-Scheme, BLisp
   - Let the target handle memory or go GC-free

4. **Host Platform**: Lux, Coalton
   - Leverage JVM/host GC

### Type System Evolution

1. **Classic HM**: Pre-Scheme, Coalton, Indigo
2. **HM + Type Classes**: Coalton, Lux (as modules)
3. **Effect Systems**: FX-87 (pioneering), BLisp (modern)
4. **Substructural**: Linear Lisp, MOLL, Alms
5. **Dependent Types**: Lux (partial support)

### Academic Significance

- **FX-87**: First effect system (POPL 1988)
- **Linear Lisp**: GC-free via linear logic (1992)
- **Alms**: Practical affine types (POPL 2011)
- **Pre-Scheme**: Systems programming Lisp (1986)

## Further Reading

### Papers

1. Baker, H.G. "Lively Linear Lisp — 'Look Ma, No Garbage!'" (1992)
2. Lucassen, J.M. et al. "Polymorphic Effect Systems" POPL 1988
3. Tov, J. & Pucella, R. "Practical Affine Types" POPL 2011
4. Kelsey & Rees, "The Scheme 48 Implementation"

### Online Resources

- Henry Baker's Archive: http://home.pipeline.com/~hbaker1/
- Coalton Blog: https://coalton-lang.github.io/
- Pre-Scheme Restoration: https://prescheme.org/
- Lux Documentation: https://luxlang.github.io/lux/

---

## Contributing

Found another obscure statically typed Lisp? Please add it! Requirements:

- Must be statically typed to the core (no optional typing)
- Must use s-expressions or Lisp syntax
- Preference for academically interesting type systems
- Bonus points for obscurity and weirdness

