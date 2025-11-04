use crate::error::{HprofError, Result};
use crate::record::{HeapDumpTag, Record, RecordTag};
use crate::types::*;
use std::io::{Read, BufReader};

/// Streaming HPROF parser with constant memory usage
pub struct HprofParser<R: Read> {
    reader: BufReader<R>,
    header: HprofHeader,
    in_heap_dump: bool,
    heap_dump_bytes_remaining: u32,
}

impl<R: Read> HprofParser<R> {
    /// Create a new parser from a reader
    pub fn new(reader: R) -> Result<Self> {
        let mut reader = BufReader::new(reader);
        let header = Self::parse_header(&mut reader)?;

        Ok(Self {
            reader,
            header,
            in_heap_dump: false,
            heap_dump_bytes_remaining: 0,
        })
    }

    /// Get the header information
    pub fn header(&self) -> &HprofHeader {
        &self.header
    }

    /// Parse the HPROF header
    fn parse_header(reader: &mut BufReader<R>) -> Result<HprofHeader> {
        // Read null-terminated version string
        let mut version_bytes = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte)?;
            if byte[0] == 0 {
                break;
            }
            version_bytes.push(byte[0]);
        }

        let version = String::from_utf8(version_bytes)
            .map_err(|_| HprofError::InvalidHeader("Invalid version string".to_string()))?;

        if !version.starts_with("JAVA PROFILE 1.0.") {
            return Err(HprofError::InvalidHeader(format!(
                "Unknown version: {}",
                version
            )));
        }

        // Read identifier size
        let id_size = read_u4(reader)?;
        if id_size != 4 && id_size != 8 {
            return Err(HprofError::InvalidIdSize(id_size));
        }

        // Read timestamp (high and low words)
        let timestamp_high = read_u4(reader)? as u64;
        let timestamp_low = read_u4(reader)? as u64;
        let timestamp = (timestamp_high << 32) | timestamp_low;

        Ok(HprofHeader {
            version,
            id_size,
            timestamp,
        })
    }

    /// Read the next record from the stream
    pub fn next_record(&mut self) -> Result<Option<Record>> {
        // If we're in a heap dump segment, read sub-records
        if self.in_heap_dump && self.heap_dump_bytes_remaining > 0 {
            return self.read_heap_dump_subrecord();
        }

        // Otherwise read a top-level record
        let tag_byte = match read_u1(&mut self.reader) {
            Ok(b) => b,
            Err(HprofError::UnexpectedEof) => return Ok(None),
            Err(e) => return Err(e),
        };

        let time_offset = read_u4(&mut self.reader)?;
        let length = read_u4(&mut self.reader)?;

        let tag = RecordTag::from_u8(tag_byte);

        match tag {
            Some(RecordTag::String) => self.read_string(length),
            Some(RecordTag::LoadClass) => self.read_load_class(),
            Some(RecordTag::UnloadClass) => self.read_unload_class(),
            Some(RecordTag::Frame) => self.read_frame(),
            Some(RecordTag::Trace) => self.read_trace(length),
            Some(RecordTag::StartThread) => self.read_start_thread(),
            Some(RecordTag::EndThread) => self.read_end_thread(),
            Some(RecordTag::HeapSummary) => self.read_heap_summary(),
            Some(RecordTag::HeapDump) | Some(RecordTag::HeapDumpSegment) => {
                self.in_heap_dump = true;
                self.heap_dump_bytes_remaining = length;
                self.read_heap_dump_subrecord()
            }
            Some(RecordTag::HeapDumpEnd) => {
                self.in_heap_dump = false;
                self.heap_dump_bytes_remaining = 0;
                Ok(Some(Record::HeapDumpEnd))
            }
            _ => {
                // Skip unknown records
                skip_bytes(&mut self.reader, length as usize)?;
                Ok(Some(Record::Unknown {
                    tag: tag_byte,
                    time_offset,
                    length,
                }))
            }
        }
    }

    fn read_string(&mut self, length: u32) -> Result<Option<Record>> {
        let id = self.read_id()?;
        let text_len = length - self.header.id_size;
        let mut text_bytes = vec![0u8; text_len as usize];
        self.reader.read_exact(&mut text_bytes)?;

        // Use lossy UTF-8 conversion to handle invalid sequences
        // Real-world heap dumps may contain invalid UTF-8
        let text = String::from_utf8_lossy(&text_bytes).into_owned();

        Ok(Some(Record::String { id, text }))
    }

    fn read_load_class(&mut self) -> Result<Option<Record>> {
        let class_serial = read_u4(&mut self.reader)?;
        let object_id = self.read_id()?;
        let stack_trace_serial = read_u4(&mut self.reader)?;
        let class_name_id = self.read_id()?;

        Ok(Some(Record::LoadClass {
            class_serial,
            object_id,
            stack_trace_serial,
            class_name_id,
        }))
    }

    fn read_unload_class(&mut self) -> Result<Option<Record>> {
        let class_serial = read_u4(&mut self.reader)?;
        Ok(Some(Record::UnloadClass { class_serial }))
    }

    fn read_frame(&mut self) -> Result<Option<Record>> {
        let frame_id = self.read_id()?;
        let method_name_id = self.read_id()?;
        let method_signature_id = self.read_id()?;
        let source_file_id = self.read_id()?;
        let class_serial = read_u4(&mut self.reader)?;
        let line_number = read_i4(&mut self.reader)?;

        Ok(Some(Record::Frame(StackFrame {
            frame_id,
            method_name_id,
            method_signature_id,
            source_file_id,
            class_serial,
            line_number,
        })))
    }

    fn read_trace(&mut self, _length: u32) -> Result<Option<Record>> {
        let serial = read_u4(&mut self.reader)?;
        let thread_serial = read_u4(&mut self.reader)?;
        let num_frames = read_u4(&mut self.reader)?;

        let mut frame_ids = Vec::with_capacity(num_frames as usize);
        for _ in 0..num_frames {
            frame_ids.push(self.read_id()?);
        }

        Ok(Some(Record::Trace(StackTrace {
            serial,
            thread_serial,
            frame_ids,
        })))
    }

    fn read_start_thread(&mut self) -> Result<Option<Record>> {
        let thread_serial = read_u4(&mut self.reader)?;
        let object_id = self.read_id()?;
        let stack_trace_serial = read_u4(&mut self.reader)?;
        let thread_name_id = self.read_id()?;
        let thread_group_name_id = self.read_id()?;
        let thread_parent_group_name_id = self.read_id()?;

        Ok(Some(Record::StartThread {
            thread_serial,
            object_id,
            stack_trace_serial,
            thread_name_id,
            thread_group_name_id,
            thread_parent_group_name_id,
        }))
    }

    fn read_end_thread(&mut self) -> Result<Option<Record>> {
        let thread_serial = read_u4(&mut self.reader)?;
        Ok(Some(Record::EndThread { thread_serial }))
    }

    fn read_heap_summary(&mut self) -> Result<Option<Record>> {
        let total_live_bytes = read_u4(&mut self.reader)?;
        let total_live_instances = read_u4(&mut self.reader)?;
        let total_bytes_allocated = read_u8(&mut self.reader)?;
        let total_instances_allocated = read_u8(&mut self.reader)?;

        Ok(Some(Record::HeapSummary {
            total_live_bytes,
            total_live_instances,
            total_bytes_allocated,
            total_instances_allocated,
        }))
    }

    fn read_heap_dump_subrecord(&mut self) -> Result<Option<Record>> {
        if self.heap_dump_bytes_remaining == 0 {
            self.in_heap_dump = false;
            return self.next_record();
        }

        let tag_byte = read_u1(&mut self.reader)?;
        self.heap_dump_bytes_remaining -= 1;

        let tag = HeapDumpTag::from_u8(tag_byte);

        match tag {
            Some(HeapDumpTag::RootUnknown) => self.read_root_unknown(),
            Some(HeapDumpTag::RootJniGlobal) => self.read_root_jni_global(),
            Some(HeapDumpTag::RootJniLocal) => self.read_root_jni_local(),
            Some(HeapDumpTag::RootJavaFrame) => self.read_root_java_frame(),
            Some(HeapDumpTag::RootNativeStack) => self.read_root_native_stack(),
            Some(HeapDumpTag::RootStickyClass) => self.read_root_sticky_class(),
            Some(HeapDumpTag::RootThreadBlock) => self.read_root_thread_block(),
            Some(HeapDumpTag::RootMonitorUsed) => self.read_root_monitor_used(),
            Some(HeapDumpTag::RootThreadObj) => self.read_root_thread_obj(),
            Some(HeapDumpTag::ClassDump) => self.read_class_dump(),
            Some(HeapDumpTag::InstanceDump) => self.read_instance_dump(),
            Some(HeapDumpTag::ObjectArrayDump) => self.read_object_array_dump(),
            Some(HeapDumpTag::PrimitiveArrayDump) => self.read_primitive_array_dump(),
            None => {
                // Unknown sub-record, try to continue
                Err(HprofError::InvalidTag(tag_byte))
            }
        }
    }

    fn read_root_unknown(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        self.heap_dump_bytes_remaining -= self.header.id_size;
        Ok(Some(Record::Root(RootType::Unknown { object_id })))
    }

    fn read_root_jni_global(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let jni_ref_id = self.read_id()?;
        self.heap_dump_bytes_remaining -= self.header.id_size * 2;
        Ok(Some(Record::Root(RootType::JniGlobal {
            object_id,
            jni_ref_id,
        })))
    }

    fn read_root_jni_local(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let thread_serial = read_u4(&mut self.reader)?;
        let frame_number = read_u4(&mut self.reader)?;
        self.heap_dump_bytes_remaining -= self.header.id_size + 8;
        Ok(Some(Record::Root(RootType::JniLocal {
            object_id,
            thread_serial,
            frame_number,
        })))
    }

    fn read_root_java_frame(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let thread_serial = read_u4(&mut self.reader)?;
        let frame_number = read_u4(&mut self.reader)?;
        self.heap_dump_bytes_remaining -= self.header.id_size + 8;
        Ok(Some(Record::Root(RootType::JavaFrame {
            object_id,
            thread_serial,
            frame_number,
        })))
    }

    fn read_root_native_stack(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let thread_serial = read_u4(&mut self.reader)?;
        self.heap_dump_bytes_remaining -= self.header.id_size + 4;
        Ok(Some(Record::Root(RootType::NativeStack {
            object_id,
            thread_serial,
        })))
    }

    fn read_root_sticky_class(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        self.heap_dump_bytes_remaining -= self.header.id_size;
        Ok(Some(Record::Root(RootType::StickyClass { object_id })))
    }

    fn read_root_thread_block(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let thread_serial = read_u4(&mut self.reader)?;
        self.heap_dump_bytes_remaining -= self.header.id_size + 4;
        Ok(Some(Record::Root(RootType::ThreadBlock {
            object_id,
            thread_serial,
        })))
    }

    fn read_root_monitor_used(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        self.heap_dump_bytes_remaining -= self.header.id_size;
        Ok(Some(Record::Root(RootType::MonitorUsed { object_id })))
    }

    fn read_root_thread_obj(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let thread_serial = read_u4(&mut self.reader)?;
        let stack_trace_serial = read_u4(&mut self.reader)?;
        self.heap_dump_bytes_remaining -= self.header.id_size + 8;
        Ok(Some(Record::Root(RootType::ThreadObject {
            object_id,
            thread_serial,
            stack_trace_serial,
        })))
    }

    fn read_class_dump(&mut self) -> Result<Option<Record>> {
        let mut bytes_consumed = 0u32;

        let object_id = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let stack_trace_serial = read_u4(&mut self.reader)?;
        bytes_consumed += 4;

        let super_class_id = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let class_loader_id = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let _signers_id = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let _protection_domain_id = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let _reserved1 = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let _reserved2 = self.read_id()?;
        bytes_consumed += self.header.id_size;

        let instance_size = read_u4(&mut self.reader)?;
        bytes_consumed += 4;

        // Constant pool
        let constant_pool_size = read_u2(&mut self.reader)?;
        bytes_consumed += 2;

        for _ in 0..constant_pool_size {
            let _index = read_u2(&mut self.reader)?;
            bytes_consumed += 2;

            let type_byte = read_u1(&mut self.reader)?;
            bytes_consumed += 1;

            let prim_type = PrimitiveType::from_u8(type_byte)
                .ok_or(HprofError::InvalidRecord(format!("Invalid type: {}", type_byte)))?;
            let size = prim_type.size(self.header.id_size);
            skip_bytes(&mut self.reader, size as usize)?;
            bytes_consumed += size;
        }

        // Static fields
        let num_static_fields = read_u2(&mut self.reader)?;
        bytes_consumed += 2;

        let mut static_fields = Vec::new();
        for _ in 0..num_static_fields {
            let name_id = self.read_id()?;
            bytes_consumed += self.header.id_size;

            let type_byte = read_u1(&mut self.reader)?;
            bytes_consumed += 1;

            let prim_type = PrimitiveType::from_u8(type_byte)
                .ok_or(HprofError::InvalidRecord(format!("Invalid type: {}", type_byte)))?;
            let size = prim_type.size(self.header.id_size);
            let mut value = vec![0u8; size as usize];
            self.reader.read_exact(&mut value)?;
            bytes_consumed += size;
            static_fields.push((name_id, prim_type, value));
        }

        // Instance fields
        let num_instance_fields = read_u2(&mut self.reader)?;
        bytes_consumed += 2;

        let mut instance_fields = Vec::new();
        for _ in 0..num_instance_fields {
            let name_id = self.read_id()?;
            bytes_consumed += self.header.id_size;

            let type_byte = read_u1(&mut self.reader)?;
            bytes_consumed += 1;

            let field_type = PrimitiveType::from_u8(type_byte)
                .ok_or(HprofError::InvalidRecord(format!("Invalid type: {}", type_byte)))?;
            instance_fields.push(FieldInfo {
                name_id,
                field_type,
            });
        }

        // Update remaining bytes
        self.heap_dump_bytes_remaining -= bytes_consumed;

        let class_name_id = 0; // We don't have this from class dump directly

        Ok(Some(Record::ClassDump(ClassInfo {
            object_id,
            stack_trace_serial,
            class_name_id,
            super_class_id,
            class_loader_id,
            instance_size,
            static_fields,
            instance_fields,
        })))
    }

    fn read_instance_dump(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let stack_trace_serial = read_u4(&mut self.reader)?;
        let class_object_id = self.read_id()?;
        let num_bytes = read_u4(&mut self.reader)?;

        let mut data = vec![0u8; num_bytes as usize];
        self.reader.read_exact(&mut data)?;

        self.heap_dump_bytes_remaining -= self.header.id_size * 2 + 8 + num_bytes;

        Ok(Some(Record::InstanceDump {
            object_id,
            stack_trace_serial,
            class_object_id,
            data,
        }))
    }

    fn read_object_array_dump(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let stack_trace_serial = read_u4(&mut self.reader)?;
        let num_elements = read_u4(&mut self.reader)?;
        let class_object_id = self.read_id()?;

        let mut elements = Vec::with_capacity(num_elements as usize);
        for _ in 0..num_elements {
            elements.push(self.read_id()?);
        }

        self.heap_dump_bytes_remaining -=
            self.header.id_size * 2 + 8 + (num_elements * self.header.id_size);

        Ok(Some(Record::ObjectArrayDump {
            object_id,
            stack_trace_serial,
            class_object_id,
            elements,
        }))
    }

    fn read_primitive_array_dump(&mut self) -> Result<Option<Record>> {
        let object_id = self.read_id()?;
        let stack_trace_serial = read_u4(&mut self.reader)?;
        let num_elements = read_u4(&mut self.reader)?;
        let element_type_byte = read_u1(&mut self.reader)?;
        let element_type = PrimitiveType::from_u8(element_type_byte)
            .ok_or(HprofError::InvalidRecord(format!(
                "Invalid element type: {}",
                element_type_byte
            )))?;

        let element_size = element_type.size(self.header.id_size);
        let total_size = num_elements * element_size;
        let mut elements = vec![0u8; total_size as usize];
        self.reader.read_exact(&mut elements)?;

        self.heap_dump_bytes_remaining -= self.header.id_size + 8 + 1 + total_size;

        Ok(Some(Record::PrimitiveArrayDump {
            object_id,
            stack_trace_serial,
            element_type,
            elements,
        }))
    }

    fn read_id(&mut self) -> Result<ObjectId> {
        if self.header.id_size == 4 {
            Ok(read_u4(&mut self.reader)? as u64)
        } else {
            read_u8(&mut self.reader)
        }
    }
}

// Helper functions for reading binary data
fn read_u1<R: Read>(reader: &mut R) -> Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            HprofError::UnexpectedEof
        } else {
            HprofError::Io(e)
        }
    })?;
    Ok(buf[0])
}

fn read_u2<R: Read>(reader: &mut R) -> Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

fn read_u4<R: Read>(reader: &mut R) -> Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

fn read_i4<R: Read>(reader: &mut R) -> Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_be_bytes(buf))
}

fn read_u8<R: Read>(reader: &mut R) -> Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_be_bytes(buf))
}

fn skip_bytes<R: Read>(reader: &mut R, count: usize) -> Result<()> {
    let mut buf = vec![0u8; count.min(8192)];
    let mut remaining = count;
    while remaining > 0 {
        let to_read = remaining.min(buf.len());
        reader.read_exact(&mut buf[..to_read])?;
        remaining -= to_read;
    }
    Ok(())
}
