//! # HPROF Parser
//!
//! A streaming parser for Java HPROF heap dump files.
//! Designed for constant memory usage to handle large heap dumps.
//!
//! ## Example
//! ```no_run
//! use hprof_parser::{HprofParser, Record};
//! use std::fs::File;
//!
//! let file = File::open("heap.hprof").unwrap();
//! let mut parser = HprofParser::new(file).unwrap();
//!
//! while let Some(record) = parser.next_record().unwrap() {
//!     match record {
//!         Record::LoadClass { class_name, .. } => {
//!             println!("Loaded class: {}", class_name);
//!         }
//!         _ => {}
//!     }
//! }
//! ```

pub mod error;
pub mod parser;
pub mod record;
pub mod types;
pub mod explorer;

pub use error::{HprofError, Result};
pub use parser::HprofParser;
pub use record::Record;
pub use types::*;
pub use explorer::HeapExplorer;
