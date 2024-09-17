fn main() {
    println!("cargo:rustc-link-search=native=./build/gblibstdc++");
    println!("cargo:rustc-link-lib=static=gblibstdc++");

    let headers = ["include/kernel/process.hpp"];

    let bindings = bindgen::Builder::default()
        .use_core()
        .ctypes_prefix("core::ffi")
        .headers(headers)
        .clang_arg("-I./gblibstdc++/include")
        .clang_arg("-I./gblibc/include")
        .clang_arg("-I./include")
        .clang_arg("-std=c++20")
        .opaque_type("std::.*")
        .enable_cxx_namespaces()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = std::path::PathBuf::from(std::env::var("PWD").unwrap());
    bindings
        .write_to_file(out_path.join("src/bindings.rs"))
        .expect("Couldn't write bindings!");
}
