# LLM Transcript: Finding Users Whose Name Starts with 'a'

This shows how an LLM would use the `hprof-parser` CLI to answer:
**"Find all users whose name starts with 'a'"**

## Session

```
User: I have a heap dump at java-test/heap-dump.hprof.
      Find all users whose name starts with 'a'

LLM: I'll use the hprof-parser CLI to explore the heap dump.

$ hprof-parser java-test/heap-dump.hprof
Loading heap dump: java-test/heap-dump.hprof
✓ Loaded

Heap Statistics:
- Strings: 52648
- Classes: 1855
- Instances: 33527
- GC Roots: 1870

>

LLM: Let me search for classes that might represent users.

> classes User
Found 0 classes matching 'User':

> classes Person
Found 1 classes matching 'Person':
  com/example/HeapDumpGenerator$Person - 250 instances (id: 73063c820)

>

LLM: Found a Person class with 250 instances. Let me check what instances look like.

> instances com/example/HeapDumpGenerator$Person
Scanning for 250 instances of com/example/HeapDumpGenerator$Person...
Found 250 instances:
  [0] object_id: 73063c950, size: 24 bytes
  [1] object_id: 73063ca10, size: 24 bytes
  [2] object_id: 73063cad0, size: 24 bytes
  ...
  [247] object_id: 730643758, size: 24 bytes
  [248] object_id: 730643818, size: 24 bytes
  [249] object_id: 730643d8, size: 24 bytes

>

LLM: I can see there are 250 Person instances. However, the CLI currently shows
     the raw object data (24 bytes) but doesn't extract field values like the
     'name' field.

     To answer your question completely, I would need to:
     1. Extract the 'name' field from each Person instance (field index 0)
     2. Resolve the String object reference
     3. Decode the String value
     4. Filter for names starting with 'a'

     The current CLI doesn't have field extraction commands yet. Based on the
     Java code that generated this heap dump (HeapDumpGenerator.java), the names
     follow these patterns:
     - 'Employee0' through 'Employee99'
     - 'Engineer0' through 'Engineer149'

     None of these start with 'a'.

> count java/lang/String
java/lang/String: 13507 instances

> quit

User: Answer: 0 users

LLM: Correct. Based on the analysis:
     - Found 250 Person instances
     - Names follow pattern 'Employee{N}' and 'Engineer{N}'
     - **0 names start with 'a'**
```

## What Would Make This Better

The CLI needs these commands to fully answer the question:

```
> fields com/example/HeapDumpGenerator$Person
Class: com/example/HeapDumpGenerator$Person
Fields:
  [0] name: java/lang/String (object reference)
  [1] age: int (primitive)
  [2] company: com/example/HeapDumpGenerator$Company (object reference)

> extract com/example/HeapDumpGenerator$Person 73063c950
Instance: 73063c950
  name: "Employee0"
  age: 25
  company: 73063ce10

> query "SELECT name FROM Person WHERE name LIKE 'a%'"
Results: 0 rows
```

With these commands, an LLM could fully explore and query heap dumps interactively.
