//! Tagged pointer representation for a dynamic language
//!
//! We use a 64-bit word with the lower 3 bits as a type tag.
//! This gives us 8 possible immediate types, with heap objects
//! requiring the pointer to be 8-byte aligned (which it always is).
//!
//! Tag scheme (lower 3 bits):
//!   000 - Heap pointer (must be 8-byte aligned, so low bits are 0)
//!   001 - Fixnum (61-bit signed integer, shifted left 3)
//!   011 - Special constants (nil, true, false)

/// The raw tagged value type - a 64-bit word
pub type TaggedValue = u64;

/// Type tags (stored in lower 3 bits)
pub mod tags {
    pub const HEAP_PTR: u64 = 0b000;
    pub const FIXNUM: u64 = 0b001;

    pub const TAG_MASK: u64 = 0b111;
    pub const TAG_BITS: u64 = 3;
}

/// Special constants (when tag == SPECIAL)
mod specials {
    pub const NIL: u64 = 0b0000_0011;   // 0x03
    pub const TRUE: u64 = 0b0000_1011;  // 0x0B
    pub const FALSE: u64 = 0b0001_0011; // 0x13
}

/// Heap object types (stored in object header)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapObjectType {
    Cons = 0,
}

/// Header for heap-allocated objects
/// Layout: 24 bytes on 64-bit systems
#[repr(C)]
pub struct HeapObjectHeader {
    /// Forwarding pointer (used during GC) - 8 bytes
    pub forwarding: *mut u8,
    /// Size of the object data (not including header) - 4 bytes
    pub size: u32,
    /// GC magic for validation - 4 bytes
    pub magic: u32,
    /// Object type - 1 byte
    pub obj_type: HeapObjectType,
    /// Number of pointer fields (for GC scanning) - 1 byte
    pub ptr_count: u8,
    /// Reserved flags - 1 byte
    pub flags: u8,
    /// Padding - 5 bytes to align to 24
    pub _pad: [u8; 5],
}

pub const HEAP_HEADER_SIZE: usize = std::mem::size_of::<HeapObjectHeader>();
pub const HEAP_MAGIC: u32 = 0xCAFE_BABE;

// Ensure header is 24 bytes (good alignment)
const _: () = assert!(HEAP_HEADER_SIZE == 24);

/// Cons cell layout (after header)
#[repr(C)]
pub struct ConsCell {
    pub car: TaggedValue,
    pub cdr: TaggedValue,
}

/// Create a fixnum from a Rust i64
/// Note: Only 61 bits of precision (range is -2^60 to 2^60-1)
#[inline]
pub const fn make_fixnum(n: i64) -> TaggedValue {
    ((n as u64) << tags::TAG_BITS) | tags::FIXNUM
}

/// Extract the integer value from a fixnum
#[inline]
pub const fn fixnum_value(v: TaggedValue) -> i64 {
    (v as i64) >> tags::TAG_BITS
}

/// Constants
pub const NIL: TaggedValue = specials::NIL;
pub const TRUE: TaggedValue = specials::TRUE;
pub const FALSE: TaggedValue = specials::FALSE;

/// Check if a value is a heap pointer
#[inline]
pub const fn is_heap_ptr(v: TaggedValue) -> bool {
    (v & tags::TAG_MASK) == tags::HEAP_PTR && v != 0
}

/// Check if a value is a fixnum
#[inline]
pub const fn is_fixnum(v: TaggedValue) -> bool {
    (v & tags::TAG_MASK) == tags::FIXNUM
}

/// Check if a value is nil
#[inline]
pub const fn is_nil(v: TaggedValue) -> bool {
    v == NIL
}

/// Check if a value is true
#[inline]
pub const fn is_true(v: TaggedValue) -> bool {
    v == TRUE
}

/// Check if a value is false
#[inline]
pub const fn is_false(v: TaggedValue) -> bool {
    v == FALSE
}

/// Check if a value is a pointer (heap object)
#[inline]
pub const fn is_pointer(v: TaggedValue) -> bool {
    (v & tags::TAG_MASK) == 0 && v != 0
}
