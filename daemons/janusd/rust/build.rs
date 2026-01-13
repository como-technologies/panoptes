// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile v1 protobuf definitions (for compatibility)
    // Path is relative to the crate root: daemons/janusd/rust
    // Proto files are at: proto/janus/v1/janus.proto
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["../../../proto/janus/v1/janus.proto"],
            &["../../../proto"],
        )?;

    // Compile v2 protobuf definitions (Rust-only features)
    // Adds: Extended ProcessInfo, UpdateGuard RPC, UpdatePolicy RPC
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["../../../proto/janus/v2/janus.proto"],
            &["../../../proto"],
        )?;

    Ok(())
}
