use crate::types::*;

/// Top-level record tags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordTag {
    String = 0x01,
    LoadClass = 0x02,
    UnloadClass = 0x03,
    Frame = 0x04,
    Trace = 0x05,
    AllocSites = 0x06,
    HeapSummary = 0x07,
    StartThread = 0x0a,
    EndThread = 0x0b,
    HeapDump = 0x0c,
    CpuSamples = 0x0d,
    ControlSettings = 0x0e,
    HeapDumpSegment = 0x1c,
    HeapDumpEnd = 0x2c,
}

impl RecordTag {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(RecordTag::String),
            0x02 => Some(RecordTag::LoadClass),
            0x03 => Some(RecordTag::UnloadClass),
            0x04 => Some(RecordTag::Frame),
            0x05 => Some(RecordTag::Trace),
            0x06 => Some(RecordTag::AllocSites),
            0x07 => Some(RecordTag::HeapSummary),
            0x0a => Some(RecordTag::StartThread),
            0x0b => Some(RecordTag::EndThread),
            0x0c => Some(RecordTag::HeapDump),
            0x0d => Some(RecordTag::CpuSamples),
            0x0e => Some(RecordTag::ControlSettings),
            0x1c => Some(RecordTag::HeapDumpSegment),
            0x2c => Some(RecordTag::HeapDumpEnd),
            _ => None,
        }
    }
}

/// Heap dump sub-record tags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapDumpTag {
    RootUnknown = 0xff,
    RootJniGlobal = 0x01,
    RootJniLocal = 0x02,
    RootJavaFrame = 0x03,
    RootNativeStack = 0x04,
    RootStickyClass = 0x05,
    RootThreadBlock = 0x06,
    RootMonitorUsed = 0x07,
    RootThreadObj = 0x08,
    ClassDump = 0x20,
    InstanceDump = 0x21,
    ObjectArrayDump = 0x22,
    PrimitiveArrayDump = 0x23,
}

impl HeapDumpTag {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0xff => Some(HeapDumpTag::RootUnknown),
            0x01 => Some(HeapDumpTag::RootJniGlobal),
            0x02 => Some(HeapDumpTag::RootJniLocal),
            0x03 => Some(HeapDumpTag::RootJavaFrame),
            0x04 => Some(HeapDumpTag::RootNativeStack),
            0x05 => Some(HeapDumpTag::RootStickyClass),
            0x06 => Some(HeapDumpTag::RootThreadBlock),
            0x07 => Some(HeapDumpTag::RootMonitorUsed),
            0x08 => Some(HeapDumpTag::RootThreadObj),
            0x20 => Some(HeapDumpTag::ClassDump),
            0x21 => Some(HeapDumpTag::InstanceDump),
            0x22 => Some(HeapDumpTag::ObjectArrayDump),
            0x23 => Some(HeapDumpTag::PrimitiveArrayDump),
            _ => None,
        }
    }
}

/// A record from the HPROF file
#[derive(Debug, Clone)]
pub enum Record {
    /// UTF-8 string
    String {
        id: ObjectId,
        text: String,
    },

    /// Class loaded
    LoadClass {
        class_serial: u32,
        object_id: ObjectId,
        stack_trace_serial: u32,
        class_name_id: ObjectId,
    },

    /// Class unloaded
    UnloadClass {
        class_serial: u32,
    },

    /// Stack frame
    Frame(StackFrame),

    /// Stack trace
    Trace(StackTrace),

    /// Start of a thread
    StartThread {
        thread_serial: u32,
        object_id: ObjectId,
        stack_trace_serial: u32,
        thread_name_id: ObjectId,
        thread_group_name_id: ObjectId,
        thread_parent_group_name_id: ObjectId,
    },

    /// End of a thread
    EndThread {
        thread_serial: u32,
    },

    /// Heap summary
    HeapSummary {
        total_live_bytes: u32,
        total_live_instances: u32,
        total_bytes_allocated: u64,
        total_instances_allocated: u64,
    },

    /// GC Root
    Root(RootType),

    /// Class dump
    ClassDump(ClassInfo),

    /// Object instance dump
    InstanceDump {
        object_id: ObjectId,
        stack_trace_serial: u32,
        class_object_id: ObjectId,
        data: Vec<u8>,
    },

    /// Object array dump
    ObjectArrayDump {
        object_id: ObjectId,
        stack_trace_serial: u32,
        class_object_id: ObjectId,
        elements: Vec<ObjectId>,
    },

    /// Primitive array dump
    PrimitiveArrayDump {
        object_id: ObjectId,
        stack_trace_serial: u32,
        element_type: PrimitiveType,
        elements: Vec<u8>,
    },

    /// Heap dump end marker
    HeapDumpEnd,

    /// Unknown or unhandled record type
    Unknown {
        tag: u8,
        time_offset: u32,
        length: u32,
    },
}
