
#include <stdint.h>

// Forward declaration - implemented in Rust
extern void* gc_alloc(uint64_t size);

// Re-export with proper calling convention
void* gc_alloc_wrapper(uint64_t size) {
    return gc_alloc(size);
}
