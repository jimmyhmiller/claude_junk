fn main() {
    // On macOS, export the runtime symbols so JIT'd code can find them
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_cons_raw_mmtk");
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_gc_mmtk");
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_car");
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_cdr");
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_print");
        println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_rt_print_list");
    }

    // On Linux, use -rdynamic to export all symbols
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg=-rdynamic");
    }
}
