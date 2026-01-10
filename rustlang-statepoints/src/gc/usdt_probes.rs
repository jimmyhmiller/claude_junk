//! USDT probe stubs
//!
//! These are no-op stubs for the probe functions. The full implementation
//! would use the `usdt` crate for DTrace integration.

#![allow(dead_code)]

#[inline]
pub fn fire_gc_start(_gc_number: usize) {}

#[inline]
pub fn fire_gc_end(_gc_number: usize, _objects_copied: usize) {}

#[inline]
pub fn fire_gc_minor_start(_gc_number: usize) {}

#[inline]
pub fn fire_gc_minor_end(_gc_number: usize) {}

#[inline]
pub fn fire_gc_full_start(_gc_number: usize) {}

#[inline]
pub fn fire_gc_full_end(_gc_number: usize) {}
