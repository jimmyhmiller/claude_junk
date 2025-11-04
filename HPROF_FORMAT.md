# HPROF Binary Format Specification

## Overview
HPROF is a binary format for Java heap dumps created by the JVM. This document describes the format for implementing a streaming parser.

## File Structure

### Header (Variable length)
```
"JAVA PROFILE 1.0.1\0" or "JAVA PROFILE 1.0.2\0" (null-terminated string)
u4: identifier size in bytes (typically 4 or 8)
u8: high word of timestamp (milliseconds since epoch)
u8: low word of timestamp
```

### Records
After the header, the file contains a series of records:
```
u1: tag (record type)
u4: time offset in microseconds
u4: length of record body in bytes
[record body - length bytes]
```

## Record Tags

### Top-Level Tags
- `0x01` STRING - UTF8 string
- `0x02` LOAD CLASS - class loaded
- `0x03` UNLOAD CLASS - class unloaded
- `0x04` FRAME - stack frame
- `0x05` TRACE - stack trace
- `0x06` ALLOC SITES - allocation sites
- `0x07` HEAP SUMMARY - heap summary
- `0x0a` START THREAD - thread start
- `0x0b` END THREAD - thread end
- `0x0c` HEAP DUMP - heap dump (old format)
- `0x0d` CPU SAMPLES - CPU samples
- `0x0e` CONTROL SETTINGS - control settings
- `0x1c` HEAP DUMP SEGMENT - heap dump segment (modern format)
- `0x2c` HEAP DUMP END - end of heap dump

### Heap Dump Sub-Records (within 0x0c or 0x1c)
- `0xff` ROOT UNKNOWN
- `0x01` ROOT JNI GLOBAL
- `0x02` ROOT JNI LOCAL
- `0x03` ROOT JAVA FRAME
- `0x04` ROOT NATIVE STACK
- `0x05` ROOT STICKY CLASS
- `0x06` ROOT THREAD BLOCK
- `0x07` ROOT MONITOR USED
- `0x08` ROOT THREAD OBJ
- `0x20` CLASS DUMP
- `0x21` INSTANCE DUMP
- `0x22` OBJECT ARRAY DUMP
- `0x23` PRIMITIVE ARRAY DUMP

## Streaming Considerations

For constant memory usage:
1. Parse header to get identifier size
2. Process records one at a time
3. For large heap dumps, use HEAP DUMP SEGMENT tags
4. Build indices incrementally or use external storage
5. Don't load entire heap into memory

## Key Data Structures

### String Record (0x01)
```
ID: string ID
[utf8 bytes]
```

### Load Class Record (0x02)
```
u4: class serial number
ID: class object ID
u4: stack trace serial number
ID: class name string ID
```

### Heap Dump Segment (0x1c)
Contains sub-records without their own timestamps. Parse until record length exhausted.

### Class Dump (0x20)
```
ID: class object ID
u4: stack trace serial number
ID: super class object ID
ID: class loader object ID
ID: signers object ID
ID: protection domain object ID
ID: reserved
ID: reserved
u4: instance size in bytes
u2: constant pool size
[constant pool entries]
u2: number of static fields
[static fields]
u2: number of instance fields
[instance fields]
```

### Instance Dump (0x21)
```
ID: object ID
u4: stack trace serial number
ID: class object ID
u4: number of bytes that follow
[instance field values]
```

### Object Array Dump (0x22)
```
ID: array object ID
u4: stack trace serial number
u4: number of elements
ID: array class object ID
[ID]*: elements
```

### Primitive Array Dump (0x23)
```
ID: array object ID
u4: stack trace serial number
u4: number of elements
u1: element type
[elements]
```

## Element Types
- 2 = object
- 4 = boolean
- 5 = char
- 6 = float
- 7 = double
- 8 = byte
- 9 = short
- 10 = int
- 11 = long
