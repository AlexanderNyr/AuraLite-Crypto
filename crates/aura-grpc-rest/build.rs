fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    // Cargo serializes build-script environment for this process; no application
    // process reads it. Rust 2024 marks environment mutation unsafe globally.
    unsafe { std::env::set_var("PROTOC", protoc) };
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aura/api/v1/service.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/aura/api/v1/service.proto");
    Ok(())
}
