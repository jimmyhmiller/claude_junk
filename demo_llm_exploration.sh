#!/bin/bash
# LLM-Driven Heap Dump Exploration Demo
# This script demonstrates how an LLM can interactively explore a Java heap dump
# using only CLI commands, without writing any code.

HPROF_FILE="java-test/concurrent-map-heap-dump.hprof"
CLI="./target/release/hprof-parser"

echo "========================================="
echo "LLM-Driven Heap Dump Exploration Demo"
echo "========================================="
echo ""

echo "Step 1: Find ConcurrentHashMap classes"
echo "Command: classes ConcurrentHashMap"
echo "---"
echo "classes ConcurrentHashMap" | $CLI $HPROF_FILE 2>&1 | grep -A 5 "Found.*classes"
echo ""

echo "Step 2: List ConcurrentHashMap instances"
echo "Command: instances java/util/concurrent/ConcurrentHashMap"
echo "---"
echo "instances java/util/concurrent/ConcurrentHashMap" | $CLI $HPROF_FILE 2>&1 | grep -A 10 "Scanning for"
echo ""

echo "Step 3: Inspect a ConcurrentHashMap instance"
echo "Command: inspect 7302072a0"
echo "---"
echo "inspect 7302072a0" | $CLI $HPROF_FILE 2>&1 | grep -A 15 "Instance 7302072a0"
echo ""

echo "Step 4: Extract the 'table' field (index 0)"
echo "Command: extract 7302072a0 0"
echo "---"
echo "extract 7302072a0 0" | $CLI $HPROF_FILE 2>&1 | grep -A 5 "Instance: 7302072a0"
echo ""

echo "Step 5: List ConcurrentHashMap\$Node instances"
echo "Command: instances java/util/concurrent/ConcurrentHashMap\$Node"
echo "---"
echo 'instances java/util/concurrent/ConcurrentHashMap$Node' | $CLI $HPROF_FILE 2>&1 | grep -A 10 "Scanning for"
echo ""

echo "Step 6: Inspect a Node to see key/value structure"
echo "Command: inspect 7300001e0"
echo "---"
echo "inspect 7300001e0" | $CLI $HPROF_FILE 2>&1 | grep -A 10 "Instance 7300001e0"
echo ""

echo "Step 7: Extract the 'key' field (index 1)"
echo "Command: extract 7300001e0 1"
echo "---"
echo "extract 7300001e0 1" | $CLI $HPROF_FILE 2>&1 | grep -A 5 "Field \[1\]"
echo ""

echo "Step 8: Extract the 'val' field (index 2)"
echo "Command: extract 7300001e0 2"
echo "---"
echo "extract 7300001e0 2" | $CLI $HPROF_FILE 2>&1 | grep -A 5 "Field \[2\]"
echo ""

echo "========================================="
echo "Summary"
echo "========================================="
echo ""
echo "The CLI tool successfully enables LLM-driven exploration:"
echo "  ✓ Find classes by pattern"
echo "  ✓ List all instances of a class"
echo "  ✓ Inspect instance fields with values"
echo "  ✓ Extract specific fields with type info"
echo "  ✓ Auto-extract String values"
echo "  ✓ Navigate object graph via object IDs"
echo ""
echo "An LLM can now explore Java heap dumps interactively"
echo "without writing any code - just using simple commands!"
