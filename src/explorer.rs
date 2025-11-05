use crate::error::Result;
use crate::parser::HprofParser;
use crate::record::Record;
use crate::types::*;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// High-level API for exploring heap dumps
/// Designed to be LLM-friendly with simple queries
///
/// Architecture:
/// - Stores metadata only (classes, strings, counts)
/// - Does NOT store instance data in memory
/// - Each query scans through the file linearly
/// - Simple, works for any dump size
pub struct HeapExplorer {
    file_path: PathBuf,
    header: HprofHeader,
    strings: HashMap<ObjectId, String>,
    classes: HashMap<ObjectId, ClassInfo>,
    class_names: HashMap<ObjectId, ObjectId>,
    instance_counts: HashMap<ObjectId, usize>,
    roots: Vec<RootType>,
    loaded_classes: HashMap<u32, LoadedClassInfo>,
}

#[derive(Debug, Clone)]
pub struct LoadedClassInfo {
    pub class_serial: u32,
    pub object_id: ObjectId,
    pub class_name_id: ObjectId,
}

#[derive(Debug, Clone)]
pub struct InstanceInfo {
    pub object_id: ObjectId,
    pub class_object_id: ObjectId,
    pub data: Vec<u8>,
}

impl HeapExplorer {
    /// Create a new heap explorer from a file path
    /// Does an initial scan to build metadata (classes, strings, counts)
    /// Instance data is NOT stored - queries scan the file on-demand
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref().to_path_buf();

        // First pass: build metadata
        let file = File::open(&file_path)?;
        let mut parser = HprofParser::new(file)?;
        let header = parser.header().clone();

        let mut explorer = Self {
            file_path,
            header,
            strings: HashMap::new(),
            classes: HashMap::new(),
            class_names: HashMap::new(),
            instance_counts: HashMap::new(),
            roots: Vec::new(),
            loaded_classes: HashMap::new(),
        };

        // Scan to build metadata
        while let Some(record) = parser.next_record()? {
            explorer.index_metadata(record);
        }

        Ok(explorer)
    }

    /// Get the header information
    pub fn header(&self) -> &HprofHeader {
        &self.header
    }

    /// Index metadata from a record (does NOT store instance data)
    fn index_metadata(&mut self, record: Record) {
        match record {
            Record::String { id, text } => {
                self.strings.insert(id, text);
            }
            Record::LoadClass {
                class_serial,
                object_id,
                class_name_id,
                ..
            } => {
                self.class_names.insert(object_id, class_name_id);
                self.loaded_classes.insert(
                    class_serial,
                    LoadedClassInfo {
                        class_serial,
                        object_id,
                        class_name_id,
                    },
                );
            }
            Record::ClassDump(class_info) => {
                self.classes.insert(class_info.object_id, class_info);
            }
            Record::InstanceDump {
                class_object_id, ..
            } => {
                // Only track counts, don't store data
                *self.instance_counts.entry(class_object_id).or_insert(0) += 1;
            }
            Record::Root(root) => {
                self.roots.push(root);
            }
            _ => {}
        }
    }

    /// Get a string by ID
    pub fn get_string(&self, id: ObjectId) -> Option<&str> {
        self.strings.get(&id).map(|s| s.as_str())
    }

    /// Get a class by object ID
    pub fn get_class(&self, id: ObjectId) -> Option<&ClassInfo> {
        self.classes.get(&id)
    }

    /// Get class name for a class object ID
    pub fn get_class_name(&self, class_id: ObjectId) -> Option<&str> {
        self.class_names
            .get(&class_id)
            .and_then(|name_id| self.get_string(*name_id))
    }

    /// Get all GC roots
    pub fn get_roots(&self) -> &[RootType] {
        &self.roots
    }

    /// List all loaded classes with their names
    pub fn list_classes(&self) -> Vec<(ObjectId, String)> {
        let mut result = Vec::new();
        for (class_id, name_id) in &self.class_names {
            if let Some(name) = self.get_string(*name_id) {
                result.push((*class_id, name.to_string()));
            }
        }
        result.sort_by(|a, b| a.1.cmp(&b.1));
        result
    }

    /// Find classes by name pattern (case-insensitive substring match)
    pub fn find_classes(&self, pattern: &str) -> Vec<(ObjectId, String)> {
        let pattern_lower = pattern.to_lowercase();
        self.list_classes()
            .into_iter()
            .filter(|(_, name)| name.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Find a class by exact name match
    pub fn find_class_exact(&self, name: &str) -> Option<(ObjectId, String)> {
        self.list_classes()
            .into_iter()
            .find(|(_, class_name)| class_name == name)
    }

    /// Count instances by class
    /// Returns the total count from the heap dump (not just stored instances)
    pub fn count_instances_by_class(&self) -> HashMap<ObjectId, usize> {
        self.instance_counts.clone()
    }

    /// Get the total instance count for a class from the heap dump
    pub fn get_instance_count(&self, class_id: ObjectId) -> usize {
        self.instance_counts.get(&class_id).copied().unwrap_or(0)
    }

    /// Get top N classes by instance count
    pub fn top_classes_by_count(&self, n: usize) -> Vec<(String, usize)> {
        let counts = self.count_instances_by_class();
        let mut class_counts: Vec<_> = counts
            .into_iter()
            .filter_map(|(class_id, count)| {
                self.get_class_name(class_id)
                    .map(|name| (name.to_string(), count))
            })
            .collect();

        class_counts.sort_by(|a, b| b.1.cmp(&a.1));
        class_counts.truncate(n);
        class_counts
    }

    /// Get instances of a specific class by scanning the file
    /// Each call re-scans the file linearly
    pub fn get_instances_of_class(&self, class_id: ObjectId) -> Result<Vec<InstanceInfo>> {
        // Pre-allocate with exact capacity to avoid reallocations during push
        let count = self.get_instance_count(class_id);
        let mut instances = Vec::with_capacity(count);

        let file = File::open(&self.file_path)?;
        let mut parser = HprofParser::new(file)?;

        while let Some(record) = parser.next_record()? {
            if let Record::InstanceDump {
                object_id,
                class_object_id,
                data,
                ..
            } = record
            {
                if class_object_id == class_id {
                    instances.push(InstanceInfo {
                        object_id,
                        class_object_id,
                        data,
                    });
                }
            }
        }

        Ok(instances)
    }

    /// Get a specific instance by object ID
    /// Scans the file until the instance is found
    pub fn get_instance(&self, target_id: ObjectId) -> Result<Option<InstanceInfo>> {
        let file = File::open(&self.file_path)?;
        let mut parser = HprofParser::new(file)?;

        while let Some(record) = parser.next_record()? {
            if let Record::InstanceDump {
                object_id,
                class_object_id,
                data,
                ..
            } = record
            {
                if object_id == target_id {
                    return Ok(Some(InstanceInfo {
                        object_id,
                        class_object_id,
                        data,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Get statistics about the heap
    pub fn get_statistics(&self) -> HeapStatistics {
        let total_instances: usize = self.instance_counts.values().sum();
        HeapStatistics {
            total_strings: self.strings.len(),
            total_classes: self.classes.len(),
            total_instances,
            total_roots: self.roots.len(),
        }
    }

    /// Extract a field value from an instance by field index
    /// Returns the raw bytes of the field
    pub fn get_field_bytes(&self, instance: &InstanceInfo, field_index: usize) -> Option<Vec<u8>> {
        let class_info = self.get_class(instance.class_object_id)?;

        if field_index >= class_info.instance_fields.len() {
            return None;
        }

        let mut offset = 0;
        for (i, field) in class_info.instance_fields.iter().enumerate() {
            let size = field.field_type.size(self.header.id_size);

            if i == field_index {
                if offset + size as usize <= instance.data.len() {
                    return Some(instance.data[offset..offset + size as usize].to_vec());
                } else {
                    return None;
                }
            }

            offset += size as usize;
        }

        None
    }

    /// Extract an object ID from a field (for reference fields)
    pub fn get_field_object_id(&self, instance: &InstanceInfo, field_index: usize) -> Option<ObjectId> {
        let bytes = self.get_field_bytes(instance, field_index)?;
        let id_size = self.header.id_size as usize;

        if id_size == 4 && bytes.len() >= 4 {
            Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64)
        } else if id_size == 8 && bytes.len() >= 8 {
            Some(u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
            ]))
        } else {
            None
        }
    }

    /// Extract a string value from an object reference field
    /// This follows the object reference to get the string
    pub fn get_field_string(&self, instance: &InstanceInfo, field_index: usize) -> Option<&str> {
        let object_id = self.get_field_object_id(instance, field_index)?;
        if object_id == 0 {
            return None; // null reference
        }
        self.get_string(object_id)
    }

    /// Get field name by index for a given instance
    pub fn get_field_name(&self, instance: &InstanceInfo, field_index: usize) -> Option<&str> {
        let class_info = self.get_class(instance.class_object_id)?;
        let field = class_info.instance_fields.get(field_index)?;
        self.get_string(field.name_id)
    }

    /// List all field names for an instance
    pub fn get_instance_fields(&self, instance: &InstanceInfo) -> Vec<(String, PrimitiveType)> {
        let class_info = match self.get_class(instance.class_object_id) {
            Some(c) => c,
            None => return Vec::new(),
        };

        class_info.instance_fields.iter()
            .map(|field| {
                let name = self.get_string(field.name_id).unwrap_or("<unknown>").to_string();
                (name, field.field_type)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct HeapStatistics {
    pub total_strings: usize,
    pub total_classes: usize,
    pub total_instances: usize,
    pub total_roots: usize,
}

impl std::fmt::Display for HeapStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Heap Statistics:\n\
             - Strings: {}\n\
             - Classes: {}\n\
             - Instances: {}\n\
             - GC Roots: {}",
            self.total_strings, self.total_classes, self.total_instances, self.total_roots
        )
    }
}
