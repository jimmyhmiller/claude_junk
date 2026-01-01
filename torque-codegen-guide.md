# Building a Torque Code Generator for Java

This guide covers how to build your own code generator that parses V8's Torque `.tq` files and generates Java code for a JavaScript interpreter.

---

## 1. Torque Compiler Architecture

The V8 Torque compiler lives in `v8/src/torque/` and has this structure:

```
src/torque/
├── ast.h                    # AST node definitions
├── declarable.h             # Declarable types (Macro, Builtin, etc.)
├── declarations.h           # Declaration handling
├── earley-parser.h          # Generic Earley parser
├── earley-parser.cc
├── torque-parser.cc         # Torque-specific grammar rules
├── type-visitor.cc          # Type resolution
├── type-oracle.cc           # Type system
├── implementation-visitor.cc # CSA code generation
├── torque-compiler.cc       # Main compiler driver
└── torque-code-generator.cc # Output generation
```

### Key Insight: Earley Parser

Torque uses an **Earley parser** - a general-purpose parser that can handle any context-free grammar. The grammar is defined directly in C++ code using a DSL:

```cpp
// From torque-parser.cc
Symbol identifier;  // matches identifiers
Symbol name;        // composed of identifier

// Rule definition pattern:
Symbol& name = Symbol("name", Rule({&identifier}, MakeIdentifier));
```

---

## 2. Torque Grammar (Reconstructed)

Based on the V8 source, here's the approximate grammar in EBNF:

```ebnf
(* Top-level *)
File            = Declaration* ;
Declaration     = NamespaceDecl | TypeDecl | MacroDecl | BuiltinDecl
                | ExternDecl | ConstDecl | ClassDecl ;

(* Namespaces *)
NamespaceDecl   = "namespace" Identifier "{" Declaration* "}" ;

(* Type Declarations *)
TypeDecl        = "type" Identifier TypeParams? ExtendsClause?
                  GeneratesClause? ConstexprClause? ";" ;
ExtendsClause   = "extends" TypeExpr ;
GeneratesClause = "generates" StringLiteral ;
ConstexprClause = "constexpr" StringLiteral ;

(* Callable Declarations *)
MacroDecl       = Annotations? "macro" Identifier TypeParams?
                  Parameters ReturnType? Labels? Body ;
BuiltinDecl     = Annotations? "javascript"? "builtin" Identifier
                  TypeParams? Parameters ReturnType? Body ;
ExternDecl      = "extern" (MacroDecl | BuiltinDecl | RuntimeDecl) ;
RuntimeDecl     = "runtime" Identifier Parameters ReturnType? ";" ;

(* Parameters and Types *)
Parameters      = "(" ParameterList? ")" ;
ParameterList   = Parameter ("," Parameter)* ;
Parameter       = Identifier ":" TypeExpr ;
ReturnType      = ":" TypeExpr ;
Labels          = "labels" LabelList ;
LabelList       = Label ("," Label)* ;
Label           = Identifier ("(" TypeList ")")? ;

TypeExpr        = Identifier TypeArgs? | UnionType | FunctionType ;
TypeArgs        = "<" TypeList ">" ;
TypeList        = TypeExpr ("," TypeExpr)* ;
UnionType       = TypeExpr "|" TypeExpr ;

(* Statements *)
Body            = "{" Statement* "}" ;
Statement       = VarDecl | Assignment | Return | If | While | For
                | Typeswitch | Try | Block | ExprStmt | Goto | Tail ;

VarDecl         = ("let" | "const") Identifier (":" TypeExpr)? "=" Expr ";" ;
Assignment      = AssignmentExpr ";" ;
Return          = "return" Expr? ";" ;
If              = "if" "(" Expr ")" Body ("else" Body)? ;
While           = "while" "(" Expr ")" Body ;
For             = "for" "(" VarDecl? ";" Expr? ";" Expr? ")" Body ;
Typeswitch      = "typeswitch" "(" Expr ")" "{" TypeCase+ "}" ;
TypeCase        = "case" "(" Identifier ":" TypeExpr ")" ":" Body ;
Try             = "try" Body LabelBlock+ ;
LabelBlock      = "label" Identifier "(" ParameterList? ")" Body ;
Goto            = "goto" Identifier ("(" ExprList ")")? ";" ;
Tail            = "tail" CallExpr ";" ;

(* Expressions *)
Expr            = TernaryExpr ;
TernaryExpr     = LogicalOr ("?" Expr ":" Expr)? ;
LogicalOr       = LogicalAnd ("||" LogicalAnd)* ;
LogicalAnd      = BitwiseOr ("&&" BitwiseOr)* ;
BitwiseOr       = BitwiseXor ("|" BitwiseXor)* ;
BitwiseXor      = BitwiseAnd ("^" BitwiseAnd)* ;
BitwiseAnd      = Equality ("&" Equality)* ;
Equality        = Comparison (("==" | "!=") Comparison)* ;
Comparison      = Shift (("<" | ">" | "<=" | ">=") Shift)* ;
Shift           = Additive (("<<" | ">>" | ">>>") Additive)* ;
Additive        = Multiplicative (("+" | "-") Multiplicative)* ;
Multiplicative  = Unary (("*" | "/" | "%") Unary)* ;
Unary           = ("!" | "-" | "~")? Primary ;
Primary         = Literal | Identifier | CallExpr | FieldAccess
                | "(" Expr ")" | NewExpr | IntrinsicCall ;

CallExpr        = Primary "(" ExprList? ")" Labels? ;
FieldAccess     = Primary "." Identifier ;
IntrinsicCall   = "%" Identifier "(" ExprList? ")" ;
NewExpr         = "new" TypeExpr "{" FieldInit* "}" ;
FieldInit       = Identifier ":" Expr "," ;

(* Annotations *)
Annotations     = Annotation+ ;
Annotation      = "@" Identifier ("(" AnnotationArgs ")")? ;

(* Literals *)
Literal         = IntLiteral | StringLiteral | "true" | "false" ;
```

### Keywords

```
abstract    bitfield    builtin     case        class       const
constexpr   dcheck      else        enum        extends     extern
for         generates   goto        if          implicit    intrinsic
javascript  label       labels      let         macro       namespace
operator    otherwise   return      runtime     shape       struct
tail        transient   try         type        typeswitch  while
```

---

## 3. AST Node Types

From `src/torque/ast.h`, the key AST node hierarchy:

```
AstNode
├── Declaration
│   ├── TypeDeclaration
│   │   ├── AbstractTypeDeclaration
│   │   ├── TypeAliasDeclaration
│   │   └── ClassDeclaration
│   ├── CallableDeclaration
│   │   ├── MacroDeclaration
│   │   ├── BuiltinDeclaration
│   │   ├── IntrinsicDeclaration
│   │   └── ExternMacroDeclaration
│   ├── NamespaceDeclaration
│   ├── ConstDeclaration
│   └── SpecializationDeclaration
│
├── Statement
│   ├── BlockStatement
│   ├── ExpressionStatement
│   ├── IfStatement
│   ├── WhileStatement
│   ├── ForLoopStatement
│   ├── ReturnStatement
│   ├── GotoStatement
│   ├── TryLabelStatement
│   ├── VarDeclarationStatement
│   └── TypeswitchStatement
│
├── Expression
│   ├── IdentifierExpression
│   ├── StringLiteralExpression
│   ├── NumberLiteralExpression
│   ├── CallExpression
│   ├── IntrinsicCallExpression
│   ├── FieldAccessExpression
│   ├── ElementAccessExpression
│   ├── AssignmentExpression
│   ├── ConditionalExpression
│   ├── LogicalOrExpression
│   ├── LogicalAndExpression
│   ├── IncrementExpression
│   ├── UnaryExpression
│   └── BinaryExpression (includes comparison, arithmetic, etc.)
│
└── TypeExpression
    ├── BasicTypeExpression
    ├── UnionTypeExpression
    └── FunctionTypeExpression
```

---

## 4. Implementation Strategies

### Strategy A: Port the Earley Parser to Java

**Effort: Medium-High**

1. Translate `earley-parser.h/.cc` to Java
2. Translate the grammar rules from `torque-parser.cc`
3. Build Java AST node classes
4. Write a visitor pattern for code generation

```java
// Example Java Earley parser skeleton
public class TorqueParser {
    private Grammar grammar;

    public TorqueParser() {
        Symbol identifier = new Symbol("identifier", this::matchIdentifier);
        Symbol name = new Symbol("name",
            new Rule(List.of(identifier), this::makeIdentifier));
        // ... more rules
    }

    public Ast parse(String source) {
        return grammar.parse(tokenize(source));
    }
}
```

### Strategy B: Use a Parser Generator (ANTLR4)

**Effort: Medium** ⭐ Recommended

1. Write an ANTLR4 grammar for Torque
2. Generate Java parser + lexer
3. Use ANTLR's visitor/listener for code generation

```antlr
// Torque.g4
grammar Torque;

file: declaration* EOF;

declaration
    : namespaceDecl
    | typeDecl
    | macroDecl
    | builtinDecl
    | externDecl
    ;

namespaceDecl
    : 'namespace' IDENTIFIER '{' declaration* '}'
    ;

builtinDecl
    : annotation* 'javascript'? 'builtin' IDENTIFIER
      typeParams? parameters returnType? body
    ;

// ... etc

IDENTIFIER: [a-zA-Z_][a-zA-Z0-9_]*;
STRING: '"' (~["\r\n] | '\\"')* '"';
NUMBER: [0-9]+ ('.' [0-9]+)?;
```

### Strategy C: Tree-sitter Grammar

**Effort: Medium**

Use tree-sitter for incremental parsing with good error recovery:

```javascript
// grammar.js for tree-sitter
module.exports = grammar({
  name: 'torque',

  rules: {
    source_file: $ => repeat($.declaration),

    declaration: $ => choice(
      $.namespace_declaration,
      $.type_declaration,
      $.builtin_declaration,
      $.macro_declaration
    ),

    builtin_declaration: $ => seq(
      optional($.annotation),
      optional('javascript'),
      'builtin',
      $.identifier,
      optional($.type_parameters),
      $.parameters,
      optional($.return_type),
      $.body
    ),
    // ...
  }
});
```

---

## 5. Type Mapping: Torque → Java

### Core Type Mappings

| Torque Type | Java Type | Notes |
|-------------|-----------|-------|
| `Object` | `JSValue` | Base class for all JS values |
| `Smi` | `int` or `long` | Small integer (tagged) |
| `HeapNumber` | `double` | Heap-allocated number |
| `Number` | `JSNumber` | Union of Smi \| HeapNumber |
| `String` | `JSString` | JS string |
| `Boolean` | `boolean` or `JSBoolean` | |
| `Undefined` | `JSUndefined.INSTANCE` | Singleton |
| `Null` | `JSNull.INSTANCE` | Singleton |
| `JSArray` | `JSArray` | |
| `JSObject` | `JSObject` | |
| `Context` | `ExecutionContext` | Runtime context |
| `Map` | `HiddenClass` | Object shape |

### Java Base Classes

```java
public abstract class JSValue {
    public abstract JSType getType();
}

public class JSNumber extends JSValue {
    private final double value;

    public static JSNumber fromSmi(int smi) {
        return new JSNumber(smi);
    }

    public static JSNumber fromHeapNumber(double value) {
        return new JSNumber(value);
    }
}

public class JSArray extends JSObject {
    private Object[] elements;
    private int length;
}
```

---

## 6. Code Generation Patterns

### Pattern 1: Builtin → Java Method

**Torque:**
```torque
javascript builtin ArrayPush(
    context: Context, receiver: Object, ...arguments): Object {
  // ...
}
```

**Generated Java:**
```java
@JSBuiltin(name = "push", constructor = false)
public class ArrayPush extends JSBuiltinNode {
    @Override
    public Object execute(ExecutionContext context,
                          JSValue receiver,
                          JSValue[] arguments) {
        // Generated implementation
    }
}
```

### Pattern 2: Typeswitch → instanceof Chain

**Torque:**
```torque
typeswitch (value) {
  case (s: Smi): { return s + 1; }
  case (n: HeapNumber): { return n + 1.0; }
  case (Object): { return Undefined; }
}
```

**Generated Java:**
```java
if (value instanceof JSNumber num && num.isSmi()) {
    return JSNumber.fromSmi(num.toSmi() + 1);
} else if (value instanceof JSNumber num) {
    return JSNumber.fromHeapNumber(num.toDouble() + 1.0);
} else {
    return JSUndefined.INSTANCE;
}
```

### Pattern 3: Labels → Exceptions or Continuations

**Torque:**
```torque
macro TryGetElement(array: JSArray, index: Smi): Object
    labels NotFound {
  if (index >= array.length) goto NotFound;
  return array.elements[index];
}
```

**Generated Java (Exception approach):**
```java
public Object tryGetElement(JSArray array, int index)
    throws NotFoundLabel {
    if (index >= array.getLength()) {
        throw new NotFoundLabel();
    }
    return array.getElement(index);
}

// Label as exception
public static class NotFoundLabel extends ControlFlowException {}
```

**Generated Java (Continuation approach):**
```java
public sealed interface TryGetElementResult {
    record Success(Object value) implements TryGetElementResult {}
    record NotFound() implements TryGetElementResult {}
}

public TryGetElementResult tryGetElement(JSArray array, int index) {
    if (index >= array.getLength()) {
        return new TryGetElementResult.NotFound();
    }
    return new TryGetElementResult.Success(array.getElement(index));
}
```

---

## 7. Project Structure

```
torque-java-codegen/
├── grammar/
│   └── Torque.g4              # ANTLR4 grammar
├── src/main/java/
│   ├── parser/
│   │   ├── TorqueLexer.java   # Generated
│   │   ├── TorqueParser.java  # Generated
│   │   └── TorqueVisitor.java # Generated
│   ├── ast/
│   │   ├── AstNode.java
│   │   ├── Declaration.java
│   │   ├── BuiltinDeclaration.java
│   │   ├── MacroDeclaration.java
│   │   ├── Statement.java
│   │   └── Expression.java
│   ├── types/
│   │   ├── TorqueType.java
│   │   ├── TypeResolver.java
│   │   └── TypeMapper.java    # Torque → Java types
│   ├── codegen/
│   │   ├── JavaCodeGenerator.java
│   │   ├── BuiltinGenerator.java
│   │   ├── MacroGenerator.java
│   │   └── templates/         # Optional: StringTemplate/Freemarker
│   └── TorqueCompiler.java    # Main entry point
├── runtime/                    # Runtime library for generated code
│   └── src/main/java/
│       ├── JSValue.java
│       ├── JSNumber.java
│       ├── JSString.java
│       ├── JSArray.java
│       ├── JSObject.java
│       └── ExecutionContext.java
└── test/
    └── v8-builtins/           # .tq files from V8
```

---

## 8. Implementation Roadmap

### Phase 1: Parser (2-3 weeks)
- [ ] Write ANTLR4 grammar covering core syntax
- [ ] Generate parser and test against V8's `.tq` files
- [ ] Build AST classes

### Phase 2: Type System (2-3 weeks)
- [ ] Implement type resolution
- [ ] Build Torque → Java type mapper
- [ ] Handle generics and union types

### Phase 3: Basic Code Generation (3-4 weeks)
- [ ] Generate Java class skeletons for builtins
- [ ] Implement expression code generation
- [ ] Implement statement code generation
- [ ] Handle `typeswitch` and labels

### Phase 4: Runtime Library (2-3 weeks)
- [ ] Implement `JSValue` hierarchy
- [ ] Implement basic operations (ToNumber, ToString, etc.)
- [ ] Implement array/object primitives

### Phase 5: Integration (2-3 weeks)
- [ ] Compile V8's `math.tq` as proof of concept
- [ ] Run against Test262 subset
- [ ] Iterate on correctness

---

## 9. Challenges to Expect

### Challenge 1: extern Declarations
Many Torque functions are `extern` - implemented in C++. You'll need to:
- Identify which externs are needed
- Implement them manually in Java
- Or generate stubs

### Challenge 2: Intrinsics (`%Functions`)
V8 intrinsics like `%RawDownCast`, `%GetMap` access internal V8 structures. Map these to your Java runtime equivalents.

### Challenge 3: Memory Model Differences
V8's tagged pointers and hidden classes don't map directly to Java. You'll need an abstraction layer.

### Challenge 4: Performance
Generated code won't match V8's speed. Focus on correctness first, optimize hot paths later.

---

## 10. Resources

### V8 Source
- [src/torque/](https://github.com/v8/v8/tree/main/src/torque) - Torque compiler
- [src/builtins/*.tq](https://github.com/v8/v8/tree/main/src/builtins) - Builtin implementations
- [vscode-torque](https://github.com/v8/vscode-torque) - Syntax highlighting reference

### Tools
- [ANTLR4](https://www.antlr.org/) - Parser generator
- [Tree-sitter](https://tree-sitter.github.io/) - Incremental parsing
- [JavaPoet](https://github.com/square/javapoet) - Java code generation library

### Similar Projects
- [AcornJS](https://github.com/acornjs/acorn) - JS parser in JS
- [GraalJS](https://github.com/oracle/graaljs) - JS on JVM (study their builtin structure)
- [Test262](https://github.com/tc39/test262) - ECMAScript conformance test suite
