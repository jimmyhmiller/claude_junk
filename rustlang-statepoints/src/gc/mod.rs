//! Garbage Collection Module
//!
//! Provides a generational garbage collector with mark-and-sweep old generation.

use std::error::Error;

use crate::types::BuiltInTypes;

pub mod generational;
pub mod mark_and_sweep;
pub mod stack_walker;
pub mod usdt_probes;

/// Get the system page size
pub fn get_page_size() -> usize {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

/// Stack map details for a safepoint
#[derive(Debug, Clone)]
pub struct StackMapDetails {
    pub function_name: Option<String>,
    pub number_of_locals: usize,
    pub current_stack_size: usize,
    pub max_stack_size: usize,
}

pub const STACK_SIZE: usize = 1024 * 1024 * 128;

/// Stack map for tracking GC roots on the stack
#[derive(Debug, Clone)]
pub struct StackMap {
    details: Vec<(usize, StackMapDetails)>,
}

impl Default for StackMap {
    fn default() -> Self {
        Self::new()
    }
}

impl StackMap {
    pub fn new() -> Self {
        Self { details: vec![] }
    }

    pub fn find_stack_data(&self, pointer: usize) -> Option<&StackMapDetails> {
        for (key, value) in self.details.iter() {
            if *key == pointer {
                return Some(value);
            }
        }
        None
    }

    pub fn extend(&mut self, translated_stack_map: Vec<(usize, StackMapDetails)>) {
        self.details.extend(translated_stack_map);
    }

    pub fn details(&self) -> &[(usize, StackMapDetails)] {
        &self.details
    }
}

/// Options for the allocator
#[derive(Debug, Clone, Copy)]
pub struct AllocatorOptions {
    pub gc: bool,
    pub print_stats: bool,
    pub gc_always: bool,
}

impl Default for AllocatorOptions {
    fn default() -> Self {
        Self {
            gc: true,
            print_stats: false,
            gc_always: false,
        }
    }
}

/// Result of an allocation attempt
pub enum AllocateAction {
    Allocated(*const u8),
    Gc,
}

/// Trait for garbage collectors
pub trait Allocator {
    fn new(options: AllocatorOptions) -> Self;

    fn try_allocate(
        &mut self,
        words: usize,
        kind: BuiltInTypes,
    ) -> Result<AllocateAction, Box<dyn Error>>;

    fn try_allocate_zeroed(
        &mut self,
        words: usize,
        kind: BuiltInTypes,
    ) -> Result<AllocateAction, Box<dyn Error>> {
        self.try_allocate(words, kind)
    }

    fn allocate_for_runtime(&mut self, words: usize) -> Result<usize, Box<dyn Error>> {
        match self.try_allocate(words, BuiltInTypes::HeapObject)? {
            AllocateAction::Allocated(ptr) => {
                Ok(BuiltInTypes::HeapObject.tag(ptr as isize) as usize)
            }
            AllocateAction::Gc => Err("Need GC to allocate runtime object".into()),
        }
    }

    fn gc(&mut self, stack_map: &StackMap, stack_pointers: &[(usize, usize, usize)]);

    fn grow(&mut self);

    fn get_pause_pointer(&self) -> usize {
        0
    }

    fn get_allocation_options(&self) -> AllocatorOptions;

    fn write_barrier(&mut self, _object_ptr: usize, _new_value: usize) {
        // Default: no-op for non-generational GCs
    }

    fn get_card_table_biased_ptr(&self) -> *mut u8 {
        std::ptr::null_mut()
    }

    fn mark_card_unconditional(&mut self, _object_ptr: usize) {
        // Default: no-op for non-generational GCs
    }
}
