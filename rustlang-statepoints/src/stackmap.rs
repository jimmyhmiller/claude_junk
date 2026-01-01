//! LLVM Stack Map parser
//!
//! Parses the .llvm_stackmaps section from compiled object files.
//! See: https://llvm.org/docs/StackMaps.html#stack-map-format

use std::io::{Cursor, Read};

/// Function entry in the stack map
#[derive(Debug, Clone)]
pub struct StackMapFunction {
    pub address: u64,
    pub stack_size: u64,
    pub record_count: u64,
}

/// Location type for a value
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LocationType {
    Register,
    Direct,
    Indirect,
    Constant,
    ConstantIndex,
}

/// A location describing where a value lives
#[derive(Debug, Clone)]
pub struct Location {
    pub ty: LocationType,
    pub reg: u16,      // DWARF register number
    pub offset: i32,   // offset for Indirect/Direct
}

/// A single stack map record (one safepoint)
#[derive(Debug)]
pub struct StackMapRecord {
    pub instruction_offset: u32,
    /// Absolute offset from start of text section (function_address + instruction_offset)
    pub absolute_offset: u64,
    pub locations: Vec<Location>,
}

/// Parsed stack map
#[derive(Debug)]
pub struct StackMap {
    pub functions: Vec<StackMapFunction>,
    pub records: Vec<StackMapRecord>,
    pub constants: Vec<u64>,
}

impl StackMap {
    /// Parse a stack map from raw bytes
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(data);

        // Read header
        let version = read_u8(&mut cursor)?;
        if version != 3 {
            return Err(format!("Unsupported stack map version: {}", version));
        }

        let _reserved1 = read_u8(&mut cursor)?;
        let _reserved2 = read_u16(&mut cursor)?;

        let num_functions = read_u32(&mut cursor)?;
        let num_constants = read_u32(&mut cursor)?;
        let num_records = read_u32(&mut cursor)?;

        // Read function entries - we need these to compute absolute offsets
        let mut functions = Vec::with_capacity(num_functions as usize);
        for _ in 0..num_functions {
            let address = read_u64(&mut cursor)?;
            let stack_size = read_u64(&mut cursor)?;
            let record_count = read_u64(&mut cursor)?;
            functions.push(StackMapFunction { address, stack_size, record_count });
        }

        let mut constants = Vec::with_capacity(num_constants as usize);
        for _ in 0..num_constants {
            let constant = read_u64(&mut cursor)?;
            constants.push(constant);
        }

        // Build a map from record index to function address
        // Records are grouped by function, in order
        let mut record_to_function_addr = Vec::with_capacity(num_records as usize);
        for func in &functions {
            for _ in 0..func.record_count {
                record_to_function_addr.push(func.address);
            }
        }

        // Read records
        let mut records = Vec::with_capacity(num_records as usize);
        for record_idx in 0..num_records as usize {
            let _id = read_u64(&mut cursor)?;
            let instruction_offset = read_u32(&mut cursor)?;
            let _reserved = read_u16(&mut cursor)?;
            let num_locations = read_u16(&mut cursor)?;

            let mut locations = Vec::with_capacity(num_locations as usize);
            for _ in 0..num_locations {
                let ty_byte = read_u8(&mut cursor)?;
                let ty = match ty_byte {
                    0x01 => LocationType::Register,
                    0x02 => LocationType::Direct,
                    0x03 => LocationType::Indirect,
                    0x04 => LocationType::Constant,
                    0x05 => LocationType::ConstantIndex,
                    _ => return Err(format!("Unknown location type: {}", ty_byte)),
                };
                let _reserved = read_u8(&mut cursor)?;
                let _size = read_u16(&mut cursor)?;
                let reg = read_u16(&mut cursor)?;
                let _reserved2 = read_u16(&mut cursor)?;
                let offset = read_i32(&mut cursor)?;

                locations.push(Location { ty, reg, offset });
            }

            // Align to 8 bytes
            let pos = cursor.position();
            if pos % 8 != 0 {
                cursor.set_position(pos + (8 - pos % 8));
            }

            // Skip live-outs (we don't use them)
            let _padding = read_u16(&mut cursor)?;
            let num_live_outs = read_u16(&mut cursor)?;
            for _ in 0..num_live_outs {
                let _reg = read_u16(&mut cursor)?;
                let _reserved = read_u8(&mut cursor)?;
                let _size = read_u8(&mut cursor)?;
            }

            // Align to 8 bytes
            let pos = cursor.position();
            if pos % 8 != 0 {
                cursor.set_position(pos + (8 - pos % 8));
            }

            // Compute absolute offset: function_address + instruction_offset
            let function_address = record_to_function_addr.get(record_idx).copied().unwrap_or(0);
            let absolute_offset = function_address + instruction_offset as u64;

            records.push(StackMapRecord {
                instruction_offset,
                absolute_offset,
                locations,
            });
        }

        Ok(StackMap {
            functions,
            records,
            constants,
        })
    }

    /// Get GC pointer locations for a safepoint
    /// Returns (base_location, derived_location) pairs
    pub fn get_gc_locations(&self, record: &StackMapRecord) -> Vec<(Location, Location)> {
        // The first 3 locations are standard statepoint info:
        // 0: calling convention
        // 1: flags
        // 2: num deopt locations
        // Then deopt locations, then GC relocations come in pairs

        if record.locations.len() < 3 {
            return vec![];
        }

        // Skip the header locations and any deopt locations
        let num_deopt = if record.locations.len() > 2 {
            match record.locations[2].ty {
                LocationType::Constant => record.locations[2].offset as usize,
                LocationType::ConstantIndex => {
                    let idx = record.locations[2].offset;
                    if idx >= 0 {
                        self.constants
                            .get(idx as usize)
                            .copied()
                            .unwrap_or(0) as usize
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        } else {
            0
        };

        let gc_start = 3 + num_deopt;

        if gc_start >= record.locations.len() {
            return vec![];
        }

        let gc_locs = &record.locations[gc_start..];

        // GC locations come in pairs: (base, derived)
        gc_locs
            .chunks(2)
            .filter_map(|pair| {
                if pair.len() == 2 {
                    Some((pair[0].clone(), pair[1].clone()))
                } else {
                    None
                }
            })
            .collect()
    }
}

// Helper functions for reading binary data
fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut buf = [0u8; 1];
    cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf[0])
}

fn read_u16(cursor: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut buf = [0u8; 2];
    cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i32(cursor: &mut Cursor<&[u8]>) -> Result<i32, String> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(i32::from_le_bytes(buf))
}

fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(u64::from_le_bytes(buf))
}
