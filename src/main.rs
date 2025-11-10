use hprof_parser::{server::HprofServer, protocol::*, Result};
use std::env;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    match args[1].as_str() {
        "server" => {
            if args.len() < 3 {
                eprintln!("Usage: {} server <heap-dump.hprof>", args[0]);
                std::process::exit(1);
            }
            run_server(&args[2])
        }
        "find-maps" => run_client(Request::FindMaps),
        "dump-map" => {
            if args.len() < 3 {
                eprintln!("Usage: {} dump-map <object-id>", args[0]);
                std::process::exit(1);
            }
            run_client(Request::DumpMap {
                object_id: args[2].clone(),
            })
        }
        "shutdown" => run_client(Request::Shutdown),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage(&args[0]);
            std::process::exit(1);
        }
    }
}

fn run_server(hprof_path: &str) -> Result<()> {
    let mut server = HprofServer::new(hprof_path)?;
    server.run()
}

fn run_client(request: Request) -> Result<()> {
    // Find the server socket
    let socket_path = find_server_socket()?;

    // Connect to server
    let stream = UnixStream::connect(&socket_path)
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("Failed to connect to server. Is it running? Error: {}", e)
            )
        })?;

    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    // Send request
    let json = serde_json::to_string(&request)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;

    // Read response
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let response: Response = serde_json::from_str(&line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Handle response
    match response {
        Response::Ok { data } => {
            print_response_data(data)?;
        }
        Response::Error { message } => {
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn find_server_socket() -> Result<PathBuf> {
    // Look for /tmp/hprof-*.sock files
    let tmp_dir = std::fs::read_dir("/tmp")?;

    for entry in tmp_dir {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name() {
            if let Some(name_str) = name.to_str() {
                if name_str.starts_with("hprof-") && name_str.ends_with(".sock") {
                    return Ok(path);
                }
            }
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No HPROF server found. Start one with: hprof-parser server <file>"
    ).into())
}

fn print_response_data(data: ResponseData) -> Result<()> {
    match data {
        ResponseData::Maps(maps) => {
            println!("Found {} Maps:", maps.len());
            for (idx, map) in maps.iter().enumerate() {
                println!("  [{}] {} @ {} ({} entries)",
                    idx,
                    map.class_name.split('/').last().unwrap_or(&map.class_name),
                    map.object_id,
                    map.size
                );
            }
        }
        ResponseData::MapContents(contents) => {
            println!("Map contents ({} entries):", contents.len());
            for (key, value) in contents.iter() {
                println!("  \"{}\" -> \"{}\"", key, value);
            }
        }
        ResponseData::Collections(collections) => {
            println!("Found {} Collections:", collections.len());
            for (idx, coll) in collections.iter().enumerate() {
                println!("  [{}] {} @ {} ({} elements)",
                    idx,
                    coll.class_name.split('/').last().unwrap_or(&coll.class_name),
                    coll.object_id,
                    coll.size
                );
            }
        }
        ResponseData::CollectionContents(contents) => {
            println!("Collection contents ({} elements):", contents.len());
            for (idx, elem) in contents.iter().enumerate() {
                println!("  [{}] {}", idx, elem);
            }
        }
        ResponseData::Strings(strings) => {
            println!("Found {} Strings:", strings.len());
            for s in strings.iter() {
                println!("  @ {} (len {}): \"{}\"", s.object_id, s.length, s.value);
            }
        }
        ResponseData::Ack => {
            println!("OK");
        }
    }

    Ok(())
}

fn print_usage(program: &str) {
    eprintln!("HPROF Parser - Server/Client Heap Dump Explorer");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {} server <heap-dump.hprof>    Start server", program);
    eprintln!("  {} find-maps                    List all Map instances", program);
    eprintln!("  {} dump-map <object-id>         Dump all key-value pairs from a Map", program);
    eprintln!("  {} shutdown                     Shutdown the server", program);
    eprintln!();
    eprintln!("Workflow:");
    eprintln!("  1. Start server: {} server heap.hprof", program);
    eprintln!("  2. Query in another terminal:");
    eprintln!("     {} find-maps", program);
    eprintln!("     {} dump-map <id>", program);
}
