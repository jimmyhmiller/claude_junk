//! Generational Garbage Collector
//!
//! A two-generation GC with:
//! - Young generation: fast bump-pointer allocation with copying collection
//! - Old generation: mark-and-sweep with free list
//!
//! Objects are allocated in the young generation and promoted to old generation
//! after surviving a minor GC.

use std::{error::Error, ffi::c_void, io};

use libc::mprotect;

use super::get_page_size;
use super::usdt_probes;

use crate::types::{BuiltInTypes, Header, HeapObject, Word};

use super::{
    AllocateAction, Allocator, AllocatorOptions, StackMap, mark_and_sweep::MarkAndSweep,
    stack_walker::StackWalker,
};

/// Represents a reference to a GC root that needs updating after collection.
/// Points to a mutable slot holding the root value (stack slots, GlobalObjectBlock entries).
struct RootRef(*mut usize);

impl RootRef {
    /// Get the current value of this root
    fn value(&self) -> usize {
        unsafe { *self.0 }
    }
}

const DEFAULT_PAGE_COUNT: usize = 1024;
const MAX_PAGE_COUNT: usize = 1000000;

/// Card size in bytes (512 = 2^9)
const CARD_SIZE_LOG2: usize = 9;
const CARD_SIZE: usize = 1 << CARD_SIZE_LOG2;

/// Card table for write barrier tracking.
///
/// Each byte in the table represents one 512-byte "card" of the old generation heap.
/// When a heap store occurs, the card containing the destination is marked dirty.
/// During minor GC, only dirty cards need to be scanned for old-to-young references.
///
/// Card values: 0 = clean, non-zero = dirty
pub struct CardTable {
    /// The card table memory
    cards: Vec<u8>,
    /// Start address of the heap region this table covers
    heap_start: usize,
    /// Number of cards in the table
    card_count: usize,
    /// Biased pointer for fast card marking: cards.as_ptr() - (heap_start >> CARD_SIZE_LOG2)
    /// This allows codegen to compute: biased_ptr[addr >> 9] = 1
    biased_ptr: *mut u8,
    /// Track which cards have been marked dirty (for efficient iteration)
    dirty_card_indices: Vec<usize>,
}

unsafe impl Send for CardTable {}
unsafe impl Sync for CardTable {}

impl CardTable {
    /// Create a new card table covering the given heap range.
    fn new(heap_start: usize, heap_size: usize) -> Self {
        let card_count = heap_size.div_ceil(CARD_SIZE);
        let mut cards = vec![0u8; card_count];
        let biased_ptr = unsafe { cards.as_mut_ptr().sub(heap_start >> CARD_SIZE_LOG2) };
        Self {
            cards,
            heap_start,
            card_count,
            biased_ptr,
            dirty_card_indices: Vec::with_capacity(64),
        }
    }

    /// Mark the card containing the given address as dirty.
    /// This is the fast path used by generated code.
    #[inline]
    pub fn mark_dirty(&mut self, addr: usize) {
        let card_index = (addr - self.heap_start) >> CARD_SIZE_LOG2;
        if card_index < self.card_count {
            // Only add to dirty list if not already dirty
            if self.cards[card_index] == 0 {
                self.cards[card_index] = 1;
                self.dirty_card_indices.push(card_index);
            }
        }
    }

    /// Resize the card table to cover a larger heap.
    /// Called when the old generation grows.
    pub fn resize(&mut self, new_heap_size: usize) {
        let new_card_count = new_heap_size.div_ceil(CARD_SIZE);
        if new_card_count > self.card_count {
            self.cards.resize(new_card_count, 0);
            self.card_count = new_card_count;
            self.biased_ptr = unsafe {
                self.cards
                    .as_mut_ptr()
                    .sub(self.heap_start >> CARD_SIZE_LOG2)
            };
        }
    }

    /// Check if a card is dirty.
    #[inline]
    #[allow(unused)]
    pub fn is_dirty(&self, card_index: usize) -> bool {
        card_index < self.card_count && self.cards[card_index] != 0
    }

    /// Get the biased pointer for codegen.
    /// Generated code can do: biased_ptr[addr >> 9] = 1
    pub fn biased_ptr(&self) -> *mut u8 {
        self.biased_ptr
    }

    /// Get the list of dirty card indices (O(1) instead of scanning whole table).
    pub fn dirty_card_indices(&self) -> &[usize] {
        &self.dirty_card_indices
    }

    /// Clear all dirty cards and the tracking list.
    pub fn clear(&mut self) {
        for &card_index in &self.dirty_card_indices {
            self.cards[card_index] = 0;
        }
        self.dirty_card_indices.clear();
    }

    /// Check if there are any dirty cards.
    pub fn has_dirty_cards(&self) -> bool {
        !self.dirty_card_indices.is_empty()
    }
}

struct Space {
    start: *const u8,
    page_count: usize,
    allocation_offset: usize,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    #[allow(unused)]
    fn word_count(&self) -> usize {
        (self.page_count * get_page_size()) / 8
    }

    fn byte_count(&self) -> usize {
        self.page_count * get_page_size()
    }

    fn contains(&self, pointer: *const u8) -> bool {
        let start = self.start as usize;
        let end = start + self.byte_count();
        let pointer = pointer as usize;
        pointer >= start && pointer < end
    }

    /// Check if pointer is within the ALLOCATED portion of the space.
    fn contains_allocated(&self, pointer: *const u8) -> bool {
        let start = self.start as usize;
        let end = start + self.allocation_offset;
        let pointer = pointer as usize;
        pointer >= start && pointer < end
    }

    fn write_object(&mut self, offset: usize, size: Word) -> *const u8 {
        let mut heap_object = HeapObject::from_untagged(unsafe { self.start.add(offset) });
        assert!(self.contains(heap_object.get_pointer()));
        heap_object.write_header(size);
        heap_object.get_pointer()
    }

    fn write_object_zeroed(&mut self, offset: usize, size: Word) -> *const u8 {
        let mut heap_object = HeapObject::from_untagged(unsafe { self.start.add(offset) });
        assert!(self.contains(heap_object.get_pointer()));

        let header_size = if size.to_words() > Header::MAX_INLINE_SIZE {
            16
        } else {
            8
        };
        let full_size = size.to_bytes() + header_size;
        unsafe {
            std::ptr::write_bytes(self.start.add(offset) as *mut u8, 0, full_size);
        }

        heap_object.write_header(size);
        heap_object.get_pointer()
    }

    fn allocate(&mut self, size: Word) -> *const u8 {
        let offset = self.allocation_offset;
        let header_size = if size.to_words() > Header::MAX_INLINE_SIZE {
            16
        } else {
            8
        };
        let full_size = size.to_bytes() + header_size;
        let pointer = self.write_object(offset, size);
        self.increment_current_offset(full_size);
        pointer
    }

    fn allocate_zeroed(&mut self, size: Word) -> *const u8 {
        let offset = self.allocation_offset;
        let header_size = if size.to_words() > Header::MAX_INLINE_SIZE {
            16
        } else {
            8
        };
        let full_size = size.to_bytes() + header_size;
        let pointer = self.write_object_zeroed(offset, size);
        self.increment_current_offset(full_size);
        pointer
    }

    fn increment_current_offset(&mut self, size: usize) {
        self.allocation_offset += size;
    }

    fn clear(&mut self) {
        self.allocation_offset = 0;
    }

    fn new(default_page_count: usize) -> Self {
        let pre_allocated_space = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                get_page_size() * MAX_PAGE_COUNT,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        Self::commit_memory(pre_allocated_space, default_page_count * get_page_size()).unwrap();
        Self {
            start: pre_allocated_space as *const u8,
            page_count: default_page_count,
            allocation_offset: 0,
        }
    }

    fn commit_memory(addr: *mut c_void, size: usize) -> Result<(), io::Error> {
        unsafe {
            if mprotect(addr, size, libc::PROT_READ | libc::PROT_WRITE) != 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    #[allow(unused)]
    fn double_committed_memory(&mut self) {
        let new_page_count = self.page_count * 2;
        Self::commit_memory(self.start as *mut c_void, new_page_count * get_page_size()).unwrap();
        self.page_count = new_page_count;
    }

    fn can_allocate(&self, size: Word) -> bool {
        let header_size = if size.to_words() > Header::MAX_INLINE_SIZE {
            16
        } else {
            8
        };
        let alloc_size = size.to_bytes() + header_size;
        let new_offset = self.allocation_offset + alloc_size;
        if new_offset > self.byte_count() {
            return false;
        }
        true
    }
}

pub struct GenerationalGC {
    young: Space,
    old: MarkAndSweep,
    copied: Vec<HeapObject>,
    gc_count: usize,
    full_gc_frequency: usize,
    atomic_pause: [u8; 8],
    options: AllocatorOptions,
    /// Remembered set: old gen objects that contain pointers to young gen.
    remembered_set: Vec<usize>,
    /// Card table for tracking writes to old generation from generated code.
    card_table: CardTable,
}

impl Allocator for GenerationalGC {
    fn new(options: AllocatorOptions) -> Self {
        let young = Space::new(DEFAULT_PAGE_COUNT * 10);
        let old = MarkAndSweep::new_with_page_count(DEFAULT_PAGE_COUNT * 100, options);
        let card_table = CardTable::new(old.heap_start(), old.heap_size());
        Self {
            young,
            old,
            copied: vec![],
            gc_count: 0,
            full_gc_frequency: 100,
            atomic_pause: [0; 8],
            options,
            remembered_set: Vec::with_capacity(64),
            card_table,
        }
    }

    fn try_allocate(
        &mut self,
        words: usize,
        kind: BuiltInTypes,
    ) -> Result<AllocateAction, Box<dyn Error>> {
        let pointer = self.allocate_inner(words, kind)?;
        Ok(pointer)
    }

    fn try_allocate_zeroed(
        &mut self,
        words: usize,
        kind: BuiltInTypes,
    ) -> Result<AllocateAction, Box<dyn Error>> {
        let pointer = self.allocate_inner_zeroed(words, kind)?;
        Ok(pointer)
    }

    fn gc(&mut self, stack_map: &super::StackMap, stack_pointers: &[(usize, usize, usize)]) {
        if !self.options.gc {
            return;
        }
        if self.gc_count != 0 && self.gc_count % self.full_gc_frequency == 0 {
            self.gc_count = 0;
            self.full_gc(stack_map, stack_pointers);
        } else {
            self.minor_gc(stack_map, stack_pointers);
        }
        self.gc_count += 1;
    }

    fn grow(&mut self) {
        self.old.grow();
        self.card_table.resize(self.old.heap_size());
    }

    fn allocate_for_runtime(&mut self, words: usize) -> Result<usize, Box<dyn std::error::Error>> {
        match self.old.try_allocate(words, BuiltInTypes::HeapObject)? {
            super::AllocateAction::Allocated(ptr) => {
                Ok(BuiltInTypes::HeapObject.tag(ptr as isize) as usize)
            }
            super::AllocateAction::Gc => Err("Need GC to allocate runtime object".into()),
        }
    }

    #[allow(unused)]
    fn get_pause_pointer(&self) -> usize {
        self.atomic_pause.as_ptr() as usize
    }

    fn get_allocation_options(&self) -> AllocatorOptions {
        self.options
    }

    fn write_barrier(&mut self, object_ptr: usize, new_value: usize) {
        if !BuiltInTypes::is_heap_pointer(new_value) {
            return;
        }

        let new_value_untagged = BuiltInTypes::untag(new_value);

        if !self.young.contains(new_value_untagged as *const u8) {
            return;
        }

        if !BuiltInTypes::is_heap_pointer(object_ptr) {
            return;
        }

        let object_untagged = BuiltInTypes::untag(object_ptr);
        if !self.old.contains(object_untagged as *const u8) {
            return;
        }

        self.card_table.mark_dirty(object_untagged);

        if !self.remembered_set.contains(&object_ptr) {
            #[cfg(feature = "debug-gc")]
            eprintln!(
                "[GC DEBUG] write_barrier: adding old-gen object {:#x} to remembered set (points to young-gen {:#x})",
                object_ptr, new_value
            );
            self.remembered_set.push(object_ptr);
        }
    }

    fn get_card_table_biased_ptr(&self) -> *mut u8 {
        self.card_table.biased_ptr()
    }

    fn mark_card_unconditional(&mut self, object_ptr: usize) {
        if !BuiltInTypes::is_heap_pointer(object_ptr) {
            return;
        }

        let object_untagged = BuiltInTypes::untag(object_ptr);

        if self.old.contains(object_untagged as *const u8) {
            self.card_table.mark_dirty(object_untagged);
        }
    }
}

impl GenerationalGC {
    fn allocate_inner(
        &mut self,
        words: usize,
        _kind: BuiltInTypes,
    ) -> Result<AllocateAction, Box<dyn Error>> {
        let size = Word::from_word(words);
        if self.young.can_allocate(size) {
            let ptr = self.young.allocate(size);
            Ok(AllocateAction::Allocated(ptr))
        } else {
            Ok(AllocateAction::Gc)
        }
    }

    fn allocate_inner_zeroed(
        &mut self,
        words: usize,
        _kind: BuiltInTypes,
    ) -> Result<AllocateAction, Box<dyn Error>> {
        let size = Word::from_word(words);
        if self.young.can_allocate(size) {
            let ptr = self.young.allocate_zeroed(size);
            Ok(AllocateAction::Allocated(ptr))
        } else {
            Ok(AllocateAction::Gc)
        }
    }

    /// Gather stack roots as RootRefs.
    fn gather_stack_root_refs(
        &self,
        stack_map: &StackMap,
        stack_pointers: &[(usize, usize, usize)],
    ) -> (Vec<RootRef>, Vec<usize>) {
        let mut slots = Vec::new();
        let mut old_gen_values = Vec::new();

        for (stack_base, frame_pointer, gc_return_addr) in stack_pointers {
            let (young_roots, old_roots) = self.gather_stack_roots_inner(
                *stack_base,
                stack_map,
                *frame_pointer,
                *gc_return_addr,
            );

            for (slot_addr, _value) in young_roots {
                slots.push(RootRef(slot_addr as *mut usize));
            }

            old_gen_values.extend(old_roots);
        }

        (slots, old_gen_values)
    }

    fn gather_stack_roots_inner(
        &self,
        stack_base: usize,
        stack_map: &StackMap,
        frame_pointer: usize,
        gc_return_addr: usize,
    ) -> (Vec<(usize, usize)>, Vec<usize>) {
        let mut roots: Vec<(usize, usize)> = Vec::with_capacity(36);
        let mut old_gen_objects: Vec<usize> = Vec::with_capacity(16);

        StackWalker::walk_stack_roots_with_return_addr(
            stack_base,
            frame_pointer,
            gc_return_addr,
            stack_map,
            |slot_addr, slot_value| {
                let untagged = BuiltInTypes::untag(slot_value);

                if untagged == 0 {
                    return;
                }

                if self.young.contains(untagged as *const u8) {
                    assert!(
                        self.young.contains_allocated(untagged as *const u8),
                        "Young gen pointer {:#x} not in allocated region",
                        untagged
                    );
                    roots.push((slot_addr, slot_value));
                } else {
                    assert!(
                        self.old.contains(untagged as *const u8),
                        "Heap pointer {:#x} (tagged {:#x}) neither in young nor old gen. Stack slot @ {:#x}",
                        untagged,
                        slot_value,
                        slot_addr
                    );

                    old_gen_objects.push(slot_value);
                }
            },
        );

        (roots, old_gen_objects)
    }

    fn update_root(&self, root: &RootRef, new_value: usize) {
        unsafe {
            *root.0 = new_value;
        }
    }

    fn process_all_roots(&mut self, roots: Vec<RootRef>) {
        for root_ref in roots {
            let old_value = root_ref.value();

            if !BuiltInTypes::is_heap_pointer(old_value) {
                continue;
            }

            let heap_object = HeapObject::from_tagged(old_value);

            if !self.young.contains(heap_object.get_pointer()) {
                continue;
            }

            let new_value = self.copy(old_value);
            self.update_root(&root_ref, new_value);
        }
    }

    fn minor_gc(&mut self, stack_map: &StackMap, stack_pointers: &[(usize, usize, usize)]) {
        let start = std::time::Instant::now();
        usdt_probes::fire_gc_minor_start(self.gc_count);

        self.gc_count += 1;

        let (stack_roots, stack_old_gen) = self.gather_stack_root_refs(stack_map, stack_pointers);
        self.process_all_roots(stack_roots);

        for old_root in stack_old_gen {
            self.process_old_gen_object(old_root);
        }

        let remembered = std::mem::take(&mut self.remembered_set);
        #[cfg(feature = "debug-gc")]
        if !remembered.is_empty() {
            eprintln!(
                "[GC DEBUG] Processing {} remembered set entries",
                remembered.len()
            );
        }
        for old_object in remembered {
            #[cfg(feature = "debug-gc")]
            eprintln!("[GC DEBUG] Processing remembered object {:#x}", old_object);
            self.process_old_gen_object(old_object);
        }

        self.process_dirty_cards();

        self.copy_remaining();

        self.young.clear();

        self.card_table.clear();

        usdt_probes::fire_gc_minor_end(self.gc_count);
        if self.options.print_stats {
            println!("Minor gc took {:?}", start.elapsed());
        }
    }

    fn process_dirty_cards(&mut self) {
        if !self.card_table.has_dirty_cards() {
            return;
        }

        let dirty_cards: std::collections::HashSet<usize> = self
            .card_table
            .dirty_card_indices()
            .iter()
            .copied()
            .collect();

        #[cfg(feature = "debug-gc")]
        eprintln!("[GC DEBUG] Processing {} dirty cards", dirty_cards.len());

        let old_start = self.old.heap_start();
        let mut objects_to_process: Vec<usize> = Vec::new();

        self.old.walk_objects_mut(|obj_addr, heap_obj| {
            let card_index = (obj_addr - old_start) >> CARD_SIZE_LOG2;
            if dirty_cards.contains(&card_index) {
                let tagged = BuiltInTypes::HeapObject.tag(obj_addr as isize) as usize;
                objects_to_process.push(tagged);

                #[cfg(feature = "debug-gc")]
                {
                    eprintln!(
                        "[GC DEBUG] Object at {:#x} is in dirty card {}",
                        obj_addr, card_index
                    );
                    let _ = heap_obj;
                }
            }
        });

        for old_object in objects_to_process {
            self.process_old_gen_object(old_object);
        }
    }

    fn process_old_gen_object(&mut self, old_object: usize) {
        let mut heap_obj = HeapObject::from_tagged(old_object);

        let data = heap_obj.get_fields_mut();
        #[cfg(feature = "debug-gc")]
        eprintln!(
            "[GC DEBUG] process_old_gen_object {:#x}: {} fields",
            old_object,
            data.len()
        );
        for (_i, field) in data.iter_mut().enumerate() {
            if BuiltInTypes::is_heap_pointer(*field) {
                let field_obj = HeapObject::from_tagged(*field);
                let field_ptr = field_obj.get_pointer();

                if self.young.contains(field_ptr) {
                    #[cfg(feature = "debug-gc")]
                    eprintln!(
                        "[GC DEBUG]   field[{}] = {:#x} is in young gen, copying",
                        _i, *field
                    );
                    let new_value = self.copy(*field);
                    *field = new_value;
                    #[cfg(feature = "debug-gc")]
                    eprintln!("[GC DEBUG]   -> new value: {:#x}", new_value);
                }
            }
        }
    }

    fn copy(&mut self, root: usize) -> usize {
        if !BuiltInTypes::is_heap_pointer(root) {
            return root;
        }

        let heap_object = HeapObject::from_tagged(root);
        let tag = BuiltInTypes::get_kind(root);

        if !self.young.contains(heap_object.get_pointer()) {
            return root;
        }

        let untagged = heap_object.untagged();
        let pointer = untagged as *mut usize;
        let header_data = unsafe { *pointer };
        if Header::is_forwarding_bit_set(header_data) {
            return Header::clear_forwarding_bit(header_data);
        }

        let data = heap_object.get_full_object_data();
        let new_pointer = self.old.copy_data_to_offset(data);

        let new_object = HeapObject::from_untagged(new_pointer);
        self.copied.push(new_object);

        let tagged_new = tag.tag(new_pointer as isize) as usize;
        unsafe { *pointer = Header::set_forwarding_bit(tagged_new) };

        tagged_new
    }

    fn copy_remaining(&mut self) {
        #[cfg(feature = "debug-gc")]
        let mut iterations = 0;
        while let Some(mut object) = self.copied.pop() {
            #[cfg(feature = "debug-gc")]
            {
                iterations += 1;
                eprintln!(
                    "[GC DEBUG] copy_remaining iteration {}: processing object at {:#x}",
                    iterations,
                    object.untagged()
                );
            }
            for field in object.get_fields_mut().iter_mut() {
                if BuiltInTypes::is_heap_pointer(*field) {
                    let heap_obj = HeapObject::from_tagged(*field);
                    if self.young.contains(heap_obj.get_pointer()) {
                        #[cfg(feature = "debug-gc")]
                        eprintln!("[GC DEBUG]   copying young-gen field {:#x}", *field);
                        *field = self.copy(*field);
                    }
                }
            }
        }
        #[cfg(feature = "debug-gc")]
        eprintln!(
            "[GC DEBUG] copy_remaining done after {} iterations",
            iterations
        );
    }

    fn full_gc(&mut self, stack_map: &StackMap, stack_pointers: &[(usize, usize, usize)]) {
        usdt_probes::fire_gc_full_start(self.gc_count);
        self.minor_gc(stack_map, stack_pointers);
        self.old.gc(stack_map, stack_pointers);
        usdt_probes::fire_gc_full_end(self.gc_count);
    }
}
