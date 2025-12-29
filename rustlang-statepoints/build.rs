fn main() {
    // On macOS, export the runtime symbols so JIT'd code can find them
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_cons_raw_jit");
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_gc_jit");
    }

    // On Linux, use -rdynamic to export all symbols
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg=-rdynamic");
    }
}
