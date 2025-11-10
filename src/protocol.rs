use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request sent from client to server
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Request {
    /// Find all Map instances in the heap
    FindMaps,
    /// Dump all key-value pairs from a Map
    DumpMap { object_id: String },
    /// Find all Collection instances
    FindCollections,
    /// Dump all elements from a Collection
    DumpCollection { object_id: String },
    /// Find String instances matching a pattern
    FindStrings { pattern: String },
    /// Shutdown the server
    Shutdown,
}

/// Response sent from server to client
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Response {
    /// Successful response with data
    Ok { data: ResponseData },
    /// Error response
    Error { message: String },
}

/// Data payload in successful responses
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// List of Map metadata
    Maps(Vec<MapInfo>),
    /// Map contents (key-value pairs)
    MapContents(HashMap<String, String>),
    /// List of Collection metadata
    Collections(Vec<CollectionInfo>),
    /// Collection contents
    CollectionContents(Vec<String>),
    /// List of String instances
    Strings(Vec<StringInfo>),
    /// Simple acknowledgment
    Ack,
}

/// Metadata about a Map instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub object_id: String,
    pub class_name: String,
    pub size: usize,
    pub key_type: Option<String>,
    pub value_type: Option<String>,
}

/// Metadata about a Collection instance
#[derive(Debug, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub object_id: String,
    pub class_name: String,
    pub size: usize,
    pub element_type: Option<String>,
}

/// Metadata about a String instance
#[derive(Debug, Serialize, Deserialize)]
pub struct StringInfo {
    pub object_id: String,
    pub value: String,
    pub length: usize,
}
