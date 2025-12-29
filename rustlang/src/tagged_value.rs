//! Tagged pointer representation for a dynamic language
//!
//! We use a 64-bit word with the lower 3 bits as a type tag.
//! This gives us 8 possible immediate types, with heap objects
//! requiring the pointer to be 8-byte aligned (which it always is).
//!
//! Tag scheme (lower 3 bits):
//!   000 - Heap pointer (must be 8-byte aligned, so low bits are 0)
//!   001 - Fixnum (61-bit signed integer, shifted left 3)
//!   010 - Character (Unicode codepoint in upper bits)
//!   011 - Special constants (nil, true, false, undefined)
//!   100 - Symbol (index into symbol table)
//!   101 - (reserved for future use)
//!   110 - (reserved for future use)
//!   111 - (reserved for future use)

use std::fmt;

/// The raw tagged value type - a 64-bit word
pub type TaggedValue = u64;

/// Type tags (stored in lower 3 bits)
pub mod tags {
    pub const HEAP_PTR: u64 = 0b000;
    pub const FIXNUM: u64 = 0b001;
    pub const CHAR: u64 = 0b010;
    pub const SPECIAL: u64 = 0b011;
    pub const SYMBOL: u64 = 0b100;

    pub const TAG_MASK: u64 = 0b111;
    pub const TAG_BITS: u64 = 3;
}

/// Special constants (when tag == SPECIAL)
pub mod specials {
    pub const NIL: u64 = 0b0000_0011;      // 0x03
    pub const TRUE: u64 = 0b0000_1011;     // 0x0B
    pub const FALSE: u64 = 0b0001_0011;    // 0x13
    pub const UNDEFINED: u64 = 0b0001_1011; // 0x1B
}

/// Heap object types (stored in object header)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapObjectType {
    Cons = 0,
    String = 1,
    Vector = 2,
    Closure = 3,
    Symbol = 4,
    ByteArray = 5,
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

/// Create a character value
#[inline]
pub const fn make_char(c: char) -> TaggedValue {
    ((c as u64) << tags::TAG_BITS) | tags::CHAR
}

/// Extract the character from a char value
#[inline]
pub fn char_value(v: TaggedValue) -> char {
    char::from_u32((v >> tags::TAG_BITS) as u32).unwrap_or('\0')
}

/// Create a symbol (index into symbol table)
#[inline]
pub const fn make_symbol(idx: u32) -> TaggedValue {
    ((idx as u64) << tags::TAG_BITS) | tags::SYMBOL
}

/// Extract symbol index
#[inline]
pub const fn symbol_index(v: TaggedValue) -> u32 {
    (v >> tags::TAG_BITS) as u32
}

/// Constants
pub const NIL: TaggedValue = specials::NIL;
pub const TRUE: TaggedValue = specials::TRUE;
pub const FALSE: TaggedValue = specials::FALSE;
pub const UNDEFINED: TaggedValue = specials::UNDEFINED;

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

/// Check if a value is a character
#[inline]
pub const fn is_char(v: TaggedValue) -> bool {
    (v & tags::TAG_MASK) == tags::CHAR
}

/// Check if a value is a special constant
#[inline]
pub const fn is_special(v: TaggedValue) -> bool {
    (v & tags::TAG_MASK) == tags::SPECIAL
}

/// Check if a value is a symbol
#[inline]
pub const fn is_symbol(v: TaggedValue) -> bool {
    (v & tags::TAG_MASK) == tags::SYMBOL
}

/// Check if a value is nil
#[inline]
pub const fn is_nil(v: TaggedValue) -> bool {
    v == NIL
}

/// Check if a value is truthy (everything except nil and false)
#[inline]
pub const fn is_truthy(v: TaggedValue) -> bool {
    v != NIL && v != FALSE
}

/// Convert a raw pointer to a tagged heap pointer
#[inline]
pub fn ptr_to_tagged(ptr: *mut u8) -> TaggedValue {
    debug_assert!(
        (ptr as usize) & tags::TAG_MASK as usize == 0,
        "Heap pointer must be 8-byte aligned"
    );
    ptr as TaggedValue
}

/// Convert a tagged heap pointer back to a raw pointer
#[inline]
pub fn tagged_to_ptr(v: TaggedValue) -> *mut u8 {
    debug_assert!(is_heap_ptr(v), "Value is not a heap pointer");
    v as *mut u8
}

/// Get the header of a heap object
#[inline]
pub unsafe fn get_header(v: TaggedValue) -> &'static HeapObjectHeader {
    debug_assert!(is_heap_ptr(v));
    let ptr = v as *const u8;
    let header_ptr = ptr.sub(HEAP_HEADER_SIZE) as *const HeapObjectHeader;
    &*header_ptr
}

/// Get the header of a heap object (mutable)
#[inline]
pub unsafe fn get_header_mut(v: TaggedValue) -> &'static mut HeapObjectHeader {
    debug_assert!(is_heap_ptr(v));
    let ptr = v as *mut u8;
    let header_ptr = ptr.sub(HEAP_HEADER_SIZE) as *mut HeapObjectHeader;
    &mut *header_ptr
}

/// Get object type from a heap value
pub fn heap_object_type(v: TaggedValue) -> HeapObjectType {
    unsafe { get_header(v).obj_type }
}

/// Check if a value is a cons cell
pub fn is_cons(v: TaggedValue) -> bool {
    is_heap_ptr(v) && heap_object_type(v) == HeapObjectType::Cons
}

/// Get the car of a cons cell
pub fn car(v: TaggedValue) -> TaggedValue {
    debug_assert!(is_cons(v), "car: not a cons cell");
    unsafe {
        let cons = v as *const ConsCell;
        (*cons).car
    }
}

/// Get the cdr of a cons cell
pub fn cdr(v: TaggedValue) -> TaggedValue {
    debug_assert!(is_cons(v), "cdr: not a cons cell");
    unsafe {
        let cons = v as *const ConsCell;
        (*cons).cdr
    }
}

/// Set the car of a cons cell
pub fn set_car(cell: TaggedValue, value: TaggedValue) {
    debug_assert!(is_cons(cell), "set-car: not a cons cell");
    unsafe {
        let cons = cell as *mut ConsCell;
        (*cons).car = value;
    }
}

/// Set the cdr of a cons cell
pub fn set_cdr(cell: TaggedValue, value: TaggedValue) {
    debug_assert!(is_cons(cell), "set-cdr: not a cons cell");
    unsafe {
        let cons = cell as *mut ConsCell;
        (*cons).cdr = value;
    }
}

/// Display trait for TaggedValue
pub struct TaggedValueDisplay(pub TaggedValue);

impl fmt::Display for TaggedValueDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;

        if v == NIL {
            write!(f, "nil")
        } else if v == TRUE {
            write!(f, "#t")
        } else if v == FALSE {
            write!(f, "#f")
        } else if v == UNDEFINED {
            write!(f, "#<undefined>")
        } else if is_fixnum(v) {
            write!(f, "{}", fixnum_value(v))
        } else if is_char(v) {
            write!(f, "#\\{}", char_value(v))
        } else if is_symbol(v) {
            write!(f, "sym:{}", symbol_index(v))
        } else if is_heap_ptr(v) {
            let obj_type = heap_object_type(v);
            match obj_type {
                HeapObjectType::Cons => {
                    write!(f, "(")?;
                    let mut current = v;
                    let mut first = true;
                    while is_cons(current) {
                        if !first {
                            write!(f, " ")?;
                        }
                        first = false;
                        write!(f, "{}", TaggedValueDisplay(car(current)))?;
                        current = cdr(current);
                    }
                    if !is_nil(current) {
                        write!(f, " . {}", TaggedValueDisplay(current))?;
                    }
                    write!(f, ")")
                }
                HeapObjectType::String => write!(f, "#<string@{:#x}>", v),
                HeapObjectType::Vector => write!(f, "#<vector@{:#x}>", v),
                HeapObjectType::Closure => write!(f, "#<closure@{:#x}>", v),
                HeapObjectType::Symbol => write!(f, "#<symbol@{:#x}>", v),
                HeapObjectType::ByteArray => write!(f, "#<bytes@{:#x}>", v),
            }
        } else {
            write!(f, "#<unknown:{:#x}>", v)
        }
    }
}

/// Arithmetic operations on tagged values
pub mod arithmetic {
    use super::*;

    /// Add two values (fixnum + fixnum)
    pub fn add(a: TaggedValue, b: TaggedValue) -> Result<TaggedValue, &'static str> {
        if !is_fixnum(a) || !is_fixnum(b) {
            return Err("add: arguments must be fixnums");
        }
        let va = fixnum_value(a);
        let vb = fixnum_value(b);
        // Check for overflow
        match va.checked_add(vb) {
            Some(result) => Ok(make_fixnum(result)),
            None => Err("add: overflow"),
        }
    }

    /// Subtract two values
    pub fn sub(a: TaggedValue, b: TaggedValue) -> Result<TaggedValue, &'static str> {
        if !is_fixnum(a) || !is_fixnum(b) {
            return Err("sub: arguments must be fixnums");
        }
        let va = fixnum_value(a);
        let vb = fixnum_value(b);
        match va.checked_sub(vb) {
            Some(result) => Ok(make_fixnum(result)),
            None => Err("sub: overflow"),
        }
    }

    /// Multiply two values
    pub fn mul(a: TaggedValue, b: TaggedValue) -> Result<TaggedValue, &'static str> {
        if !is_fixnum(a) || !is_fixnum(b) {
            return Err("mul: arguments must be fixnums");
        }
        let va = fixnum_value(a);
        let vb = fixnum_value(b);
        match va.checked_mul(vb) {
            Some(result) => Ok(make_fixnum(result)),
            None => Err("mul: overflow"),
        }
    }

    /// Compare two values
    pub fn lt(a: TaggedValue, b: TaggedValue) -> Result<TaggedValue, &'static str> {
        if !is_fixnum(a) || !is_fixnum(b) {
            return Err("lt: arguments must be fixnums");
        }
        if fixnum_value(a) < fixnum_value(b) {
            Ok(TRUE)
        } else {
            Ok(FALSE)
        }
    }

    /// Equal comparison
    pub fn eq(a: TaggedValue, b: TaggedValue) -> TaggedValue {
        if a == b {
            TRUE
        } else {
            FALSE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixnums() {
        assert!(is_fixnum(make_fixnum(0)));
        assert!(is_fixnum(make_fixnum(42)));
        assert!(is_fixnum(make_fixnum(-1)));

        assert_eq!(fixnum_value(make_fixnum(0)), 0);
        assert_eq!(fixnum_value(make_fixnum(42)), 42);
        assert_eq!(fixnum_value(make_fixnum(-1)), -1);
        assert_eq!(fixnum_value(make_fixnum(1000000)), 1000000);
    }

    #[test]
    fn test_specials() {
        assert!(is_nil(NIL));
        assert!(!is_nil(TRUE));
        assert!(!is_nil(FALSE));

        assert!(is_special(NIL));
        assert!(is_special(TRUE));
        assert!(is_special(FALSE));

        assert!(is_truthy(TRUE));
        assert!(is_truthy(make_fixnum(0)));
        assert!(!is_truthy(NIL));
        assert!(!is_truthy(FALSE));
    }

    #[test]
    fn test_chars() {
        assert!(is_char(make_char('a')));
        assert_eq!(char_value(make_char('a')), 'a');
        assert_eq!(char_value(make_char('λ')), 'λ');
    }

    #[test]
    fn test_arithmetic() {
        use arithmetic::*;

        assert_eq!(add(make_fixnum(2), make_fixnum(3)).unwrap(), make_fixnum(5));
        assert_eq!(sub(make_fixnum(5), make_fixnum(3)).unwrap(), make_fixnum(2));
        assert_eq!(mul(make_fixnum(4), make_fixnum(3)).unwrap(), make_fixnum(12));
        assert_eq!(lt(make_fixnum(2), make_fixnum(3)).unwrap(), TRUE);
        assert_eq!(lt(make_fixnum(3), make_fixnum(2)).unwrap(), FALSE);
    }
}
