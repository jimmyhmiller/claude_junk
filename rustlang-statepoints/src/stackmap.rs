//! LLVM Stack Map parser
//!
//! Parses the .llvm_stackmaps section from compiled object files.
//! See: https://llvm.org/docs/StackMaps.html#stack-map-format

use std::io::{Cursor, Read};

/// Stack map header
#[derive(Debug)]
pub struct StackMapHeader {
    pub version: u8,
    pub num_functions: u32,
    pub num_constants: u32,
    pub num_records: u32,
}

/// Function entry in the stack map
#[derive(Debug)]
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
    pub size: u16,
    pub reg: u16,      // DWARF register number
    pub offset: i32,   // offset for Indirect/Direct
}

/// A single stack map record (one safepoint)
#[derive(Debug)]
pub struct StackMapRecord {
    pub id: u64,
    pub instruction_offset: u32,
    pub locations: Vec<Location>,
    pub live_outs: Vec<(u16, u8)>, // (reg, size)
}

/// Parsed stack map
#[derive(Debug)]
pub struct StackMap {
    pub header: StackMapHeader,
    pub functions: Vec<StackMapFunction>,
    pub constants: Vec<u64>,
    pub records: Vec<StackMapRecord>,
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

        let header = StackMapHeader {
            version,
            num_functions,
            num_constants,
            num_records,
        };

        // Read function entries
        let mut functions = Vec::with_capacity(num_functions as usize);
        for _ in 0..num_functions {
            let address = read_u64(&mut cursor)?;
            let stack_size = read_u64(&mut cursor)?;
            let record_count = read_u64(&mut cursor)?;
            functions.push(StackMapFunction {
                address,
                stack_size,
                record_count,
            });
        }

        // Read constants
        let mut constants = Vec::with_capacity(num_constants as usize);
        for _ in 0..num_constants {
            constants.push(read_u64(&mut cursor)?);
        }

        // Read records
        let mut records = Vec::with_capacity(num_records as usize);
        for _ in 0..num_records {
            let id = read_u64(&mut cursor)?;
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
                let size = read_u16(&mut cursor)?;
                let reg = read_u16(&mut cursor)?;
                let _reserved2 = read_u16(&mut cursor)?;
                let offset = read_i32(&mut cursor)?;

                locations.push(Location { ty, size, reg, offset });
            }

            // Align to 8 bytes
            let pos = cursor.position();
            if pos % 8 != 0 {
                cursor.set_position(pos + (8 - pos % 8));
            }

            // Read live-outs
            let _padding = read_u16(&mut cursor)?;
            let num_live_outs = read_u16(&mut cursor)?;
            let mut live_outs = Vec::with_capacity(num_live_outs as usize);
            for _ in 0..num_live_outs {
                let reg = read_u16(&mut cursor)?;
                let _reserved = read_u8(&mut cursor)?;
                let size = read_u8(&mut cursor)?;
                live_outs.push((reg, size));
            }

            // Align to 8 bytes
            let pos = cursor.position();
            if pos % 8 != 0 {
                cursor.set_position(pos + (8 - pos % 8));
            }

            records.push(StackMapRecord {
                id,
                instruction_offset,
                locations,
                live_outs,
            });
        }

        Ok(StackMap {
            header,
            functions,
            constants,
            records,
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
            if let LocationType::Constant = record.locations[2].ty {
                record.locations[2].offset as usize
            } else {
                0
            }
        } else {
            0
        };

        let gc_start = 3 + num_deopt;
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

/// DWARF register number to register name
/// Supports both x86-64 and arm64
pub fn dwarf_reg_name(reg: u16) -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        match reg {
            0 => "rax",
            1 => "rdx",
            2 => "rcx",
            3 => "rbx",
            4 => "rsi",
            5 => "rdi",
            6 => "rbp",
            7 => "rsp",
            8 => "r8",
            9 => "r9",
            10 => "r10",
            11 => "r11",
            12 => "r12",
            13 => "r13",
            14 => "r14",
            15 => "r15",
            _ => "unknown",
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        match reg {
            0..=28 => "x0-x28",
            29 => "fp",   // x29 = frame pointer
            30 => "lr",   // x30 = link register
            31 => "sp",   // stack pointer
            _ => "unknown",
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        "unknown"
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reg_name = dwarf_reg_name(self.reg);
        let reg_str = if reg_name == "unknown" || reg_name == "x0-x28" {
            format!("r{}", self.reg)
        } else {
            reg_name.to_string()
        };

        match self.ty {
            LocationType::Register => {
                write!(f, "{}", reg_str)
            }
            LocationType::Direct => {
                write!(f, "[{} + {}]", reg_str, self.offset)
            }
            LocationType::Indirect => {
                write!(f, "*[{} + {}]", reg_str, self.offset)
            }
            LocationType::Constant => {
                write!(f, "#{}", self.offset)
            }
            LocationType::ConstantIndex => {
                write!(f, "const[{}]", self.offset)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_display() {
        let loc = Location {
            ty: LocationType::Indirect,
            size: 8,
            reg: 7, // rsp
            offset: 0,
        };
        assert_eq!(format!("{}", loc), "*[rsp + 0]");
    }
}
