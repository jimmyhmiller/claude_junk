#!/bin/bash

set -e

echo "Compiling Java code..."
mkdir -p build/classes
javac -d build/classes src/main/java/com/example/HeapDumpGenerator.java

echo "Running heap dump generator..."
cd build/classes
java com.example.HeapDumpGenerator

echo ""
echo "Moving heap dump to project root..."
mv heap-dump.hprof ../../
cd ../..

echo "Heap dump is ready at: heap-dump.hprof"
ls -lh heap-dump.hprof
