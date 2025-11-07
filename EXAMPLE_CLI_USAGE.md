# CLI Usage Example: Exploring ConcurrentHashMap

This document demonstrates how an LLM can interactively explore a Java heap dump using the CLI tool to find keys and values in a ConcurrentHashMap.

## The Interactive CLI Tool

The HPROF parser provides a CLI tool that can be used interactively to explore heap dumps. This is designed to be **LLM-drivable** - an AI agent can use simple commands to navigate through complex object structures without writing any code.

## Available Commands

```
help                          - Show available commands
stats                         - Show heap statistics
classes <pattern>             - Find classes matching pattern
top [n]                       - Show top N classes by instance count
count <class-name>            - Show instance count for exact class name
instances <class-name>        - List all instances of exact class name
inspect <object-id-hex>       - Show fields of an instance
extract <object-id-hex> <idx> - Extract field value at index
roots <object-id-hex>         - Show GC root paths for an object
quit, exit                    - Exit the program
```

## Example: Finding ConcurrentHashMap Keys and Values

### Step 1: Find ConcurrentHashMap instances

```
> classes ConcurrentHashMap
Found 18 classes matching 'ConcurrentHashMap':
  java/util/concurrent/ConcurrentHashMap - 45 instances (id: 730487680)
  java/util/concurrent/ConcurrentHashMap$Node - 1267 instances (id: 730494938)
  ...

> instances java/util/concurrent/ConcurrentHashMap
Scanning for 45 instances of java/util/concurrent/ConcurrentHashMap...
Found 45 instances:
  [0] object_id: 7302072a0, size: 84 bytes
  [1] object_id: 730212198, size: 84 bytes
  ...
```

### Step 2: Inspect a ConcurrentHashMap instance

```
> inspect 7302072a0
Instance 7302072a0
Class: java/util/concurrent/ConcurrentHashMap
Data size: 84 bytes

Fields:
  [0] table: Object = 7302072f0
  [1] nextTable: Object = null
  [2] baseCount: Long = 47
  [3] sizeCtl: Int = 96
  [4] transferIndex: Int = 0
  [5] cellsBusy: Int = 0
  [6] counterCells: Object = null
  [7] keySet: Object = null
  [8] values: Object = null
  [9] entrySet: Object = null
```

**Analysis**: This ConcurrentHashMap has 47 entries (`baseCount = 47`). The `table` field (index 0) contains the Node array at object ID `7302072f0`.

### Step 3: Extract the table field

```
> extract 7302072a0 0
Instance: 7302072a0 (java/util/concurrent/ConcurrentHashMap)
Field [0]: table (Object)
Value: 7302072f0 (Object reference)
```

**Analysis**: The table is at object `7302072f0`. This is a Node array containing the hash buckets.

### Step 4: Find Node instances

```
> instances java/util/concurrent/ConcurrentHashMap$Node
Scanning for 1267 instances of java/util/concurrent/ConcurrentHashMap$Node...
Found 1267 instances:
  [0] object_id: 7300001e0, size: 28 bytes
  [1] object_id: 730000348, size: 28 bytes
  ...
```

### Step 5: Inspect a Node to see key/value

```
> inspect 7300001e0
Instance 7300001e0
Class: java/util/concurrent/ConcurrentHashMap$Node
Data size: 28 bytes

Fields:
  [0] hash: Int = 798282763
  [1] key: Object = 7300001c0
  [2] val: Object = 7300001c0
  [3] next: Object = null
```

**Analysis**: Each Node has:
- `hash` (int): hash code of the key
- `key` (Object): reference to the key object
- `val` (Object): reference to the value object
- `next` (Object): next node in collision chain (null if no collision)

### Step 6: Extract and follow the key

```
> extract 7300001e0 1
Instance: 7300001e0 (java/util/concurrent/ConcurrentHashMap$Node)
Field [1]: key (Object)
Value: 7300001c0 (Object reference)
  Class: jdk/internal/util/WeakReferenceKey
```

**Analysis**: This Node contains a WeakReferenceKey (from internal JVM maps). To find Nodes with String keys, we would continue inspecting other Node instances.

For a Node with String keys, we would see:

```
> extract <node-id> 1
Field [1]: key (Object)
Value: <string-id> (Object reference)
  Class: java/lang/String
  String value: "alice"
```

**Key Feature**: When `extract` encounters a String object, it automatically extracts and displays the String value!

## Why This Matters

This interactive CLI enables an LLM to:

1. **Explore without code**: Navigate heap dumps using simple commands
2. **Follow object references**: Use object IDs to traverse the object graph
3. **Extract values**: Get primitive values (int, long, etc.) and String contents
4. **Understand structure**: See field names, types, and values together

This is the foundation for **LLM-driven heap dump analysis** - an AI can now explore and debug Java applications by conversing with this CLI tool.
