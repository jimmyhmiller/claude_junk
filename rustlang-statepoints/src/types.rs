//! Types for the generational GC
//!
//! Adapted from beagle's types.rs to work with our tagging scheme.

/// Built-in types identified by tag bits
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BuiltInTypes {
    HeapObject,  // 0b000 - heap pointer (8-byte aligned)
    Fixnum,      // 0b001 - 61-bit integer
    Nil,         // special constant
}

impl BuiltInTypes {
    pub fn null_value() -> isize {
        0b011 // NIL constant from tagged_value.rs
    }

    pub fn tag(&self, value: isize) -> isize {
        match self {
            BuiltInTypes::HeapObject => value, // Heap pointers are already 8-byte aligned (tag 0)
            BuiltInTypes::Fixnum => (value << 3) | 0b001,
            BuiltInTypes::Nil => 0b011,
        }
    }

    pub fn get_tag(&self) -> isize {
        match self {
            BuiltInTypes::HeapObject => 0b000,
            BuiltInTypes::Fixnum => 0b001,
            BuiltInTypes::Nil => 0b011,
        }
    }

    pub fn untag(value: usize) -> usize {
        // For heap pointers (tag 0), the pointer is the value itself
        // For other types, we would shift right by 3
        if (value & 0b111) == 0b000 {
            value
        } else {
            value >> 3
        }
    }

    pub fn get_kind(pointer: usize) -> Self {
        if pointer == Self::null_value() as usize || pointer == 0b011 {
            return BuiltInTypes::Nil;
        }
        match pointer & 0b111 {
            0b000 => BuiltInTypes::HeapObject,
            0b001 => BuiltInTypes::Fixnum,
            _ => BuiltInTypes::Nil, // Treat unknown tags as nil/non-pointer
        }
    }

    pub fn is_heap_pointer(value: usize) -> bool {
        // A value is a heap pointer if:
        // 1. The low 3 bits are 0 (tag = 0b000)
        // 2. The value is not 0 (null pointer)
        (value & 0b111) == 0b000 && value != 0
    }

    pub fn tag_size() -> i32 {
        3
    }
}

/// Header for heap objects
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Header {
    pub type_id: u8,
    pub type_data: u32,
    pub size: u16, // Size in words. For large objects (>65535), this is 0xFFFF
    pub opaque: bool,
    pub marked: bool,
    pub large: bool, // Large object flag
}

impl Header {
    const MARKED_BIT_POSITION: u32 = 0;
    const OPAQUE_BIT_POSITION: u32 = 1;
    pub const LARGE_OBJECT_BIT_POSITION: u32 = 2;
    const FORWARDING_BIT_POSITION: u32 = 3;

    pub const MAX_INLINE_SIZE: usize = 0xFFFF;

    pub fn to_usize(self) -> usize {
        let mut data: usize = 0;
        data |= (self.type_id as usize) << 56;
        data |= (self.type_data as usize) << 24;
        data |= (self.size as usize) << 8;
        if self.opaque {
            data |= 1 << Self::OPAQUE_BIT_POSITION;
        }
        if self.marked {
            data |= 1 << Self::MARKED_BIT_POSITION;
        }
        if self.large {
            data |= 1 << Self::LARGE_OBJECT_BIT_POSITION;
        }
        data
    }

    pub fn from_usize(data: usize) -> Self {
        let type_id = (data >> 56) as u8;
        let type_data = (data >> 24) as u32;
        let size = ((data >> 8) & 0xFFFF) as u16;
        let opaque = (data & (1 << Self::OPAQUE_BIT_POSITION)) != 0;
        let marked = (data & (1 << Self::MARKED_BIT_POSITION)) != 0;
        let large = (data & (1 << Self::LARGE_OBJECT_BIT_POSITION)) != 0;
        Header {
            type_id,
            type_data,
            size,
            opaque,
            marked,
            large,
        }
    }

    pub const fn marked_bit_mask() -> usize {
        1 << Self::MARKED_BIT_POSITION
    }

    pub const fn set_marked_bit(header_value: usize) -> usize {
        header_value | Self::marked_bit_mask()
    }

    pub const fn clear_marked_bit(header_value: usize) -> usize {
        header_value & !Self::marked_bit_mask()
    }

    pub const fn is_marked_bit_set(header_value: usize) -> bool {
        (header_value & Self::marked_bit_mask()) != 0
    }

    pub const fn large_object_bit_mask() -> usize {
        1 << Self::LARGE_OBJECT_BIT_POSITION
    }

    pub const fn is_large_object_bit_set(header_value: usize) -> bool {
        (header_value & Self::large_object_bit_mask()) != 0
    }

    pub const fn forwarding_bit_mask() -> usize {
        1 << Self::FORWARDING_BIT_POSITION
    }

    pub const fn set_forwarding_bit(tagged_pointer: usize) -> usize {
        tagged_pointer | Self::forwarding_bit_mask()
    }

    pub const fn clear_forwarding_bit(tagged_pointer: usize) -> usize {
        tagged_pointer & !Self::forwarding_bit_mask()
    }

    pub const fn is_forwarding_bit_set(value: usize) -> bool {
        (value & Self::forwarding_bit_mask()) != 0
    }
}

/// HeapObject wraps a pointer to a heap-allocated object
pub struct HeapObject {
    pointer: usize,
    tagged: bool,
}

impl HeapObject {
    pub fn from_tagged(pointer: usize) -> Self {
        assert!(BuiltInTypes::is_heap_pointer(pointer));
        HeapObject {
            pointer,
            tagged: true,
        }
    }

    pub fn from_untagged(pointer: *const u8) -> Self {
        assert!(pointer as usize % 8 == 0);
        HeapObject {
            pointer: pointer as usize,
            tagged: false,
        }
    }

    pub fn untagged(&self) -> usize {
        if self.tagged {
            self.pointer // For tag 0, pointer IS the untagged value
        } else {
            self.pointer
        }
    }

    pub fn mark(&self) {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let data: usize = unsafe { *pointer };
        let marked_data = Header::set_marked_bit(data);
        unsafe { *pointer = marked_data };
    }

    pub fn marked(&self) -> bool {
        self.get_header().marked
    }

    pub fn fields_size(&self) -> usize {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let data: usize = unsafe { *pointer };
        let header = Header::from_usize(data);

        if header.large {
            let size_ptr = unsafe { pointer.add(1) };
            unsafe { *size_ptr * 8 }
        } else {
            header.size as usize * 8
        }
    }

    pub fn get_fields(&self) -> &[usize] {
        if self.is_opaque_object() {
            return &[];
        }
        let size = self.fields_size();
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let pointer = unsafe { pointer.add(self.header_size() / 8) };
        unsafe { std::slice::from_raw_parts(pointer, size / 8) }
    }

    pub fn get_fields_mut(&mut self) -> &mut [usize] {
        if self.is_opaque_object() {
            return &mut [];
        }
        let size = self.fields_size();
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let pointer = unsafe { pointer.add(self.header_size() / 8) };
        unsafe { std::slice::from_raw_parts_mut(pointer, size / 8) }
    }

    pub fn get_full_object_data(&self) -> &[u8] {
        let size = self.full_size();
        let untagged = self.untagged();
        let pointer = untagged as *mut u8;
        assert!(pointer.is_aligned());
        unsafe { std::slice::from_raw_parts(pointer, size) }
    }

    pub fn get_heap_references(&self) -> impl Iterator<Item = HeapObject> + '_ {
        let fields = self.get_fields();
        fields
            .iter()
            .filter(|_x| !self.is_opaque_object())
            .filter(|x| BuiltInTypes::is_heap_pointer(**x))
            .map(|&pointer| HeapObject::from_tagged(pointer))
    }

    pub fn unmark(&self) {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let data: usize = unsafe { *pointer };
        let unmarked_data = Header::clear_marked_bit(data);
        unsafe { *pointer = unmarked_data };
    }

    pub fn full_size(&self) -> usize {
        self.fields_size() + self.header_size()
    }

    pub fn header_size(&self) -> usize {
        let header = self.get_header();
        if header.large {
            16
        } else {
            8
        }
    }

    pub fn write_header(&mut self, field_size: Word) {
        assert!(field_size.to_bytes() % 8 == 0);
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;

        let words = field_size.to_words();
        let is_large = words > Header::MAX_INLINE_SIZE;

        let header = Header {
            type_id: 0,
            type_data: 0,
            size: if is_large { 0xFFFF } else { words as u16 },
            opaque: false,
            marked: false,
            large: is_large,
        };

        unsafe { *pointer = header.to_usize() };

        if is_large {
            let size_ptr = unsafe { pointer.add(1) };
            unsafe { *size_ptr = words };
        }
    }

    pub fn get_pointer(&self) -> *const u8 {
        let untagged = self.untagged();
        untagged as *const u8
    }

    pub fn get_field(&self, arg: usize) -> usize {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let pointer = unsafe { pointer.add(arg + self.header_size() / 8) };
        unsafe { *pointer }
    }

    pub fn get_field_ptr(&self, arg: usize) -> *mut usize {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        unsafe { pointer.add(arg + self.header_size() / 8) }
    }

    pub fn is_opaque_object(&self) -> bool {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        let data: usize = unsafe { *pointer };
        let header = Header::from_usize(data);
        header.opaque
    }

    pub fn get_header(&self) -> Header {
        let untagged = self.untagged();
        let pointer = untagged as *mut usize;
        assert!(pointer.is_aligned());
        let data: usize = unsafe { *pointer };
        Header::from_usize(data)
    }
}

/// Word type for sizes (in 8-byte words)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Word(usize);

impl Word {
    pub fn to_bytes(self) -> usize {
        self.0 * 8
    }

    pub fn from_word(size: usize) -> Word {
        Word(size)
    }

    pub fn from_bytes(len: usize) -> Word {
        Word(len / 8)
    }

    pub fn to_words(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_untag() {
        // Test heap pointer tagging (tag = 0, so value is unchanged)
        let ptr = 0x1000_0000_usize;
        let tagged = BuiltInTypes::HeapObject.tag(ptr as isize);
        assert_eq!(tagged as usize, ptr);
        assert!(BuiltInTypes::is_heap_pointer(ptr));

        // Test fixnum
        let num = 42_i64;
        let tagged = BuiltInTypes::Fixnum.tag(num as isize);
        assert!(!BuiltInTypes::is_heap_pointer(tagged as usize));
        assert_eq!(BuiltInTypes::get_kind(tagged as usize), BuiltInTypes::Fixnum);
    }

    #[test]
    fn test_header() {
        let header = Header {
            type_id: 1,
            type_data: 0x12345,
            size: 4,
            opaque: false,
            marked: false,
            large: false,
        };
        let data = header.to_usize();
        let restored = Header::from_usize(data);
        assert_eq!(header, restored);
    }
}
