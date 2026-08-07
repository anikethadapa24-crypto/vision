fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Vendored protoc: keeps the build hermetic across all 3 CI OSes
    // (windows/macos/ubuntu) without requiring a `protobuf-compiler`
    // system package on each runner.
    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;
    // SAFETY: single-threaded build script, no concurrent env access.
    unsafe {
        std::env::set_var("PROTOC", protoc_path);
    }

    tonic_prost_build::compile_protos("proto/vision.proto")?;
    Ok(())
}
