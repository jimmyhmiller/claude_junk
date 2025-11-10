use crate::explorer::{HeapExplorer, InstanceInfo};
use crate::protocol::*;
use crate::Result;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// Server that holds the heap dump in memory and serves queries
pub struct HprofServer {
    explorer: HeapExplorer,
    socket_path: PathBuf,
    map_index: Vec<MapInfo>,
}

impl HprofServer {
    /// Create a new server from a heap dump file
    pub fn new<P: AsRef<Path>>(hprof_path: P) -> Result<Self> {
        eprintln!("Loading heap dump...");
        let explorer = HeapExplorer::new(&hprof_path)?;
        eprintln!("✓ Loaded");

        // Create socket path in /tmp
        let pid = std::process::id();
        let socket_path = PathBuf::from(format!("/tmp/hprof-{}.sock", pid));

        // Remove old socket if it exists
        let _ = std::fs::remove_file(&socket_path);

        let mut server = Self {
            explorer,
            socket_path,
            map_index: Vec::new(),
        };

        // Build indexes
        eprintln!("Building indexes...");
        server.build_map_index()?;
        eprintln!("✓ Indexed {} Maps", server.map_index.len());

        Ok(server)
    }

    /// Build index of all Map instances
    fn build_map_index(&mut self) -> Result<()> {
        // Find all Map classes
        let map_classes = vec![
            "java/util/HashMap",
            "java/util/concurrent/ConcurrentHashMap",
            "java/util/TreeMap",
            "java/util/LinkedHashMap",
            "java/util/Hashtable",
            "java/util/WeakHashMap",
            "java/util/IdentityHashMap",
        ];

        for class_name in map_classes {
            if let Some((class_id, _)) = self.explorer.find_class_exact(class_name) {
                let instances = self.explorer.get_instances_of_class(class_id)?;
                for instance in instances {
                    // Get the size (number of entries)
                    let size = self.get_map_size(&instance)?;

                    self.map_index.push(MapInfo {
                        object_id: format!("{:x}", instance.object_id),
                        class_name: class_name.to_string(),
                        size,
                        key_type: None,   // TODO: infer from first entry
                        value_type: None, // TODO: infer from first entry
                    });
                }
            }
        }

        Ok(())
    }

    /// Get the number of entries in a Map
    fn get_map_size(&self, instance: &InstanceInfo) -> Result<usize> {
        // For HashMap/ConcurrentHashMap, check the 'size' field or count entries
        let fields = self.explorer.get_instance_fields(instance);

        // Look for 'size' or 'baseCount' field
        for (idx, (name, _)) in fields.iter().enumerate() {
            if name == "size" {
                if let Ok(crate::explorer::FieldValue::Int(size)) =
                    self.explorer.get_field_value(instance, idx) {
                    return Ok(size as usize);
                }
            }
            if name == "baseCount" {
                if let Ok(crate::explorer::FieldValue::Long(size)) =
                    self.explorer.get_field_value(instance, idx) {
                    return Ok(size as usize);
                }
            }
        }

        Ok(0)
    }

    /// Start the server and listen for connections
    pub fn run(&mut self) -> Result<()> {
        let listener = UnixListener::bind(&self.socket_path)?;
        eprintln!("Server listening on {}", self.socket_path.display());

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(e) = self.handle_client(stream) {
                        eprintln!("Error handling client: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Connection error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Handle a client connection
    fn handle_client(&mut self, stream: UnixStream) -> Result<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        // Read request line
        let mut line = String::new();
        reader.read_line(&mut line)?;

        // Parse request
        let request: Request = serde_json::from_str(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Handle request
        let response = match request {
            Request::FindMaps => self.handle_find_maps(),
            Request::DumpMap { object_id } => self.handle_dump_map(&object_id),
            Request::FindCollections => Response::Error {
                message: "Not implemented yet".to_string(),
            },
            Request::DumpCollection { .. } => Response::Error {
                message: "Not implemented yet".to_string(),
            },
            Request::FindStrings { .. } => Response::Error {
                message: "Not implemented yet".to_string(),
            },
            Request::Shutdown => {
                let json = serde_json::to_string(&Response::Ok {
                    data: ResponseData::Ack,
                }).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                writeln!(writer, "{}", json)?;
                writer.flush()?;
                std::process::exit(0);
            }
        };

        // Send response
        let json = serde_json::to_string(&response)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writeln!(writer, "{}", json)?;
        writer.flush()?;

        Ok(())
    }

    /// Handle find-maps request
    fn handle_find_maps(&self) -> Response {
        Response::Ok {
            data: ResponseData::Maps(self.map_index.clone()),
        }
    }

    /// Handle dump-map request
    fn handle_dump_map(&self, object_id_str: &str) -> Response {
        match self.dump_map_contents(object_id_str) {
            Ok(contents) => Response::Ok {
                data: ResponseData::MapContents(contents),
            },
            Err(e) => Response::Error {
                message: format!("Failed to dump map: {}", e),
            },
        }
    }

    /// Extract all key-value pairs from a Map
    fn dump_map_contents(&self, object_id_str: &str) -> Result<HashMap<String, String>> {
        let object_id = u64::from_str_radix(object_id_str, 16)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

        let instance = self.explorer.get_instance(object_id)?
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Map instance not found"
            ))?;

        let class_name = self.explorer.get_class_name(instance.class_object_id)
            .unwrap_or("Unknown");

        // Handle different Map implementations
        if class_name.contains("HashMap") || class_name.contains("ConcurrentHashMap") {
            self.dump_hashmap_contents(&instance)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Map type not supported yet: {}", class_name)
            ).into())
        }
    }

    /// Dump HashMap or ConcurrentHashMap contents
    fn dump_hashmap_contents(&self, map_instance: &InstanceInfo)
        -> Result<HashMap<String, String>> {
        let mut result = HashMap::new();

        // Get the 'table' field (Node[] array)
        let fields = self.explorer.get_instance_fields(map_instance);
        let table_field_idx = fields.iter().position(|(name, _)| name == "table")
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Map has no 'table' field"
            ))?;

        // Extract table array object ID
        let table_id = match self.explorer.get_field_value(map_instance, table_field_idx)? {
            crate::explorer::FieldValue::Object(id) if id != 0 => id,
            _ => return Ok(result), // Empty map
        };

        // Get the Node array
        let table_array = self.explorer.get_object_array(table_id)?
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Table is not an array"
            ))?;

        // Iterate through non-null Node entries
        for &node_id in &table_array.elements {
            if node_id == 0 {
                continue; // Skip null buckets
            }

            // Follow the linked list of Nodes
            self.dump_node_chain(node_id, &mut result)?;
        }

        Ok(result)
    }

    /// Dump a chain of Nodes (handles collision chains)
    fn dump_node_chain(&self, mut node_id: u64, result: &mut HashMap<String, String>)
        -> Result<()> {
        while node_id != 0 {
            let node = self.explorer.get_instance(node_id)?
                .ok_or_else(|| std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Node not found"
                ))?;

            // Get key and value from Node
            let fields = self.explorer.get_instance_fields(&node);

            // Find key field (index 1) and val field (index 2)
            let key_str = self.extract_field_as_string(&node, 1)?;
            let val_str = self.extract_field_as_string(&node, 2)?;

            result.insert(key_str, val_str);

            // Follow 'next' pointer (index 3)
            node_id = match self.explorer.get_field_value(&node, 3)? {
                crate::explorer::FieldValue::Object(id) => id,
                _ => 0,
            };
        }

        Ok(())
    }

    /// Extract a field value as a String representation
    fn extract_field_as_string(&self, instance: &InstanceInfo, field_idx: usize)
        -> Result<String> {
        match self.explorer.get_field_value(instance, field_idx)? {
            crate::explorer::FieldValue::Object(0) => Ok("null".to_string()),
            crate::explorer::FieldValue::Object(id) => {
                // Try to get the object and extract its value
                if let Ok(Some(obj)) = self.explorer.get_instance(id) {
                    let class_name = self.explorer.get_class_name(obj.class_object_id)
                        .unwrap_or("");

                    // If it's a String, extract the value
                    if let Ok(Some(s)) = self.explorer.extract_string_value(&obj) {
                        return Ok(s);
                    }

                    // Handle boxed primitives
                    match class_name {
                        "java/lang/Integer" | "java/lang/Long" | "java/lang/Short" | "java/lang/Byte" => {
                            // Field 0 is the 'value' field
                            if let Ok(value) = self.explorer.get_field_value(&obj, 0) {
                                return self.extract_field_as_string(&obj, 0);
                            }
                        }
                        "java/lang/Boolean" => {
                            if let Ok(value) = self.explorer.get_field_value(&obj, 0) {
                                return self.extract_field_as_string(&obj, 0);
                            }
                        }
                        "java/lang/Float" | "java/lang/Double" => {
                            if let Ok(value) = self.explorer.get_field_value(&obj, 0) {
                                return self.extract_field_as_string(&obj, 0);
                            }
                        }
                        _ => {}
                    }

                    // Otherwise, return the object ID
                    Ok(format!("@{:x}", id))
                } else {
                    Ok(format!("@{:x}", id))
                }
            }
            crate::explorer::FieldValue::Int(v) => Ok(v.to_string()),
            crate::explorer::FieldValue::Long(v) => Ok(v.to_string()),
            crate::explorer::FieldValue::Boolean(v) => Ok(v.to_string()),
            crate::explorer::FieldValue::Byte(v) => Ok(v.to_string()),
            crate::explorer::FieldValue::Short(v) => Ok(v.to_string()),
            crate::explorer::FieldValue::Char(v) => Ok(char::from_u32(v as u32)
                .unwrap_or('?')
                .to_string()),
            crate::explorer::FieldValue::Float(v) => Ok(v.to_string()),
            crate::explorer::FieldValue::Double(v) => Ok(v.to_string()),
        }
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for HprofServer {
    fn drop(&mut self) {
        // Clean up socket file
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
