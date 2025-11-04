/// Object ID type - can be 4 or 8 bytes depending on the dump
pub type ObjectId = u64;

/// Header information from the HPROF file
#[derive(Debug, Clone)]
pub struct HprofHeader {
    /// Format version (e.g., "JAVA PROFILE 1.0.2")
    pub version: String,
    /// Size of object identifiers in bytes (4 or 8)
    pub id_size: u32,
    /// Timestamp when the dump was created (milliseconds since epoch)
    pub timestamp: u64,
}

/// Type of a primitive value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Object = 2,
    Boolean = 4,
    Char = 5,
    Float = 6,
    Double = 7,
    Byte = 8,
    Short = 9,
    Int = 10,
    Long = 11,
}

impl PrimitiveType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            2 => Some(PrimitiveType::Object),
            4 => Some(PrimitiveType::Boolean),
            5 => Some(PrimitiveType::Char),
            6 => Some(PrimitiveType::Float),
            7 => Some(PrimitiveType::Double),
            8 => Some(PrimitiveType::Byte),
            9 => Some(PrimitiveType::Short),
            10 => Some(PrimitiveType::Int),
            11 => Some(PrimitiveType::Long),
            _ => None,
        }
    }

    /// Returns the size in bytes for this primitive type
    pub fn size(&self, id_size: u32) -> u32 {
        match self {
            PrimitiveType::Object => id_size,
            PrimitiveType::Boolean => 1,
            PrimitiveType::Char => 2,
            PrimitiveType::Float => 4,
            PrimitiveType::Double => 8,
            PrimitiveType::Byte => 1,
            PrimitiveType::Short => 2,
            PrimitiveType::Int => 4,
            PrimitiveType::Long => 8,
        }
    }
}

/// Field information in a class
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name_id: ObjectId,
    pub field_type: PrimitiveType,
}

/// Stack frame information
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub frame_id: ObjectId,
    pub method_name_id: ObjectId,
    pub method_signature_id: ObjectId,
    pub source_file_id: ObjectId,
    pub class_serial: u32,
    pub line_number: i32,
}

/// Stack trace information
#[derive(Debug, Clone)]
pub struct StackTrace {
    pub serial: u32,
    pub thread_serial: u32,
    pub frame_ids: Vec<ObjectId>,
}

/// Class information
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub object_id: ObjectId,
    pub stack_trace_serial: u32,
    pub class_name_id: ObjectId,
    pub super_class_id: ObjectId,
    pub class_loader_id: ObjectId,
    pub instance_size: u32,
    pub static_fields: Vec<(ObjectId, PrimitiveType, Vec<u8>)>,
    pub instance_fields: Vec<FieldInfo>,
}

/// GC Root types
#[derive(Debug, Clone)]
pub enum RootType {
    Unknown { object_id: ObjectId },
    JniGlobal { object_id: ObjectId, jni_ref_id: ObjectId },
    JniLocal { object_id: ObjectId, thread_serial: u32, frame_number: u32 },
    JavaFrame { object_id: ObjectId, thread_serial: u32, frame_number: u32 },
    NativeStack { object_id: ObjectId, thread_serial: u32 },
    StickyClass { object_id: ObjectId },
    ThreadBlock { object_id: ObjectId, thread_serial: u32 },
    MonitorUsed { object_id: ObjectId },
    ThreadObject { object_id: ObjectId, thread_serial: u32, stack_trace_serial: u32 },
}
