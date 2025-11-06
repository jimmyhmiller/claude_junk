# Real Heap Dump Debugging Scenarios

## 1. Memory Leak Detection
**Scenario**: "Why is this object not being garbage collected?"

**What we need**:
- Find what's holding references to leaked objects
- Trace paths to GC roots

**Current support**: ✅ YES
- `find_gc_root_paths()` does exactly this
- CLI: `roots <object-id>`

**Missing**:
- Can't easily identify "leaked" objects automatically
- No leak detection heuristics

---

## 2. Finding Large Objects / Memory Hogs
**Scenario**: "What's taking up all the memory? Show me the biggest objects."

**What we need**:
- Calculate object sizes (shallow size)
- Calculate retained size (how much would be freed if this object was GC'd)
- Sort by size

**Current support**: ❌ NO
- We have instance **counts** but not **sizes**
- Can't calculate retained size at all

**What needs to change**:
- Add `get_instance_size(object_id)` - walk class fields to compute size
- Add `get_class_histogram()` - return (class_name, count, total_bytes)
- Retained size requires dominator tree (complex, skip for now)

**Priority**: HIGH - this is a top use case

---

## 3. String Deduplication Analysis
**Scenario**: "I have 50,000 String instances. How many are duplicates?"

**What we need**:
- Extract String values from java/lang/String instances
- Group by value, count duplicates

**Current support**: ❌ NO
- We can find String instances but can't read their values
- String value is in a char[]/byte[] field, requires parsing

**What needs to change**:
- Add `extract_string_value(instance)` - parse String internals
  1. Get 'value' field (byte[] or char[])
  2. Get 'coder' field (Latin1 vs UTF16 in Java 9+)
  3. Decode to String
- Add CLI command: `strings <class-name>` to list all string values

**Priority**: HIGH - very common use case

---

## 4. Collection Analysis
**Scenario**: "How many empty ArrayLists do I have? What's the average size?"

**What we need**:
- Extract field values (size, capacity)
- Filter and aggregate

**Current support**: ❌ NO
- Can find ArrayList instances
- Can't extract the 'size' field value (it's an int)

**What needs to change**:
- Add `get_field_value(instance, field_index)` -> returns Value enum
  ```rust
  enum FieldValue {
      Int(i32),
      Long(i64),
      Object(ObjectId),
      // etc
  }
  ```
- Add `extract_int_field()`, `extract_object_field()`, etc.

**Priority**: MEDIUM - useful but workarounds exist

---

## 5. Class Loader Leak Detection
**Scenario**: "Why is this ClassLoader (and all its classes) still loaded?"

**What we need**:
- Find paths from ClassLoader to GC roots

**Current support**: ✅ YES
- Already works with `find_gc_root_paths()`
- ClassLoader is just another object

**Priority**: N/A - already works

---

## 6. Thread Analysis
**Scenario**: "What threads exist? What objects are they holding?"

**What we need**:
- List all Thread objects
- Show thread names
- Show what each thread stack references

**Current support**: ⚠️ PARTIAL
- We have GC root data with thread info
- Can't easily list all threads or get thread names
- Thread name is a String field in java/lang/Thread

**What needs to change**:
- Add `list_threads()` - finds java/lang/Thread instances
- Extract thread name field
- Group GC roots by thread

**Priority**: MEDIUM - useful for concurrency bugs

---

## 7. Finding Specific Objects by Field Value
**Scenario**: "Find all User objects where userId=12345"

**What we need**:
- Extract field values
- Filter by predicate

**Current support**: ❌ NO
- Can't extract or filter by field values

**What needs to change**:
- Field value extraction (see #4)
- Add filtering API or SQL-like query

**Priority**: HIGH - this is the "killer feature" for LLM-drivable exploration

**Example API**:
```rust
// Find Person where age > 30
let old_people = explorer.filter_instances(person_class_id, |inst| {
    let age = explorer.get_int_field(inst, 1)?; // field index 1 = age
    Ok(age > 30)
})?;
```

---

## 8. Dominator Analysis
**Scenario**: "What's the single object keeping this whole object graph alive?"

**What we need**:
- Compute dominator tree
- Find immediate dominator of each object

**Current support**: ❌ NO
- This is algorithmically complex
- Requires full object graph in memory

**What needs to change**:
- Build full object graph (memory intensive)
- Implement dominator tree algorithm (Lengauer-Tarjan)

**Priority**: LOW - complex algorithm, high memory usage

---

## 9. Duplicate Object Detection
**Scenario**: "How many Person objects have identical field values?"

**What we need**:
- Extract all field values
- Hash/compare objects
- Group identical ones

**Current support**: ❌ NO
- Can't extract or compare field values

**What needs to change**:
- Field value extraction
- Object comparison logic

**Priority**: LOW - niche use case

---

## 10. Arbitrary Path Finding
**Scenario**: "Show me all paths from Object A to Object B"

**What we need**:
- Bidirectional BFS between any two objects

**Current support**: ⚠️ PARTIAL
- Can find paths to GC roots (one direction)
- Can't find arbitrary A->B paths

**What needs to change**:
- Add `find_paths_between(from_id, to_id, max_paths, max_depth)`

**Priority**: LOW - less common than GC root paths

---

## 11. Histogram with Sizes
**Scenario**: "Show me classes sorted by total memory usage"

**What we need**:
- Instance count per class (we have this ✅)
- Total bytes per class (don't have this ❌)

**Current support**: ⚠️ PARTIAL
- `top_classes_by_count()` shows counts only

**What needs to change**:
- Calculate sizes (see #2)
- Add `top_classes_by_size()`

**Priority**: HIGH - combines with #2

---

## 12. Reference Histogram
**Scenario**: "What objects reference the most other objects? (high fan-out)"

**What we need**:
- Count outgoing references per object
- Identify highly-connected objects

**Current support**: ❌ NO
- We don't track outgoing reference counts

**What needs to change**:
- During metadata scan, count references per object
- Store in HashMap<ObjectId, usize>

**Priority**: LOW - advanced use case

---

## Summary: What Should We Build?

### CRITICAL (must have):
1. ✅ **GC root paths** - DONE
2. ❌ **Object size calculation** - calculate shallow size per object
3. ❌ **String value extraction** - read String contents
4. ❌ **Field value extraction** - read int, long, object fields
5. ❌ **Filtering/querying** - find objects by field values

### HIGH VALUE (should have):
6. ❌ **Size histogram** - classes sorted by total memory
7. ⚠️ **Thread analysis** - list threads, show what they hold
8. ❌ **Collection helpers** - isEmpty, size, capacity

### NICE TO HAVE (could defer):
9. ❌ **Duplicate detection**
10. ❌ **Arbitrary path finding**
11. ❌ **Reference counting**

### TOO COMPLEX (skip for now):
12. ❌ **Dominator tree analysis** - requires significant algorithm work
13. ❌ **Retained size** - depends on dominators

---

## Honest Assessment

**What we're good at**:
- ✅ Finding classes and instances
- ✅ Counting instances
- ✅ GC root path tracing
- ✅ Fast linear scanning
- ✅ Constant memory usage

**What we're missing**:
- ❌ Can't read field values (biggest gap!)
- ❌ Can't calculate sizes
- ❌ Can't filter/query by values
- ❌ Can't extract String contents

**The #1 blocker**: **Field value extraction**

Without this, we can't:
- Find objects by field values
- Extract String contents
- Analyze collections (size, isEmpty)
- Do meaningful filtering

**Recommendation**: Focus on field value extraction next. This unlocks most real debugging scenarios.
