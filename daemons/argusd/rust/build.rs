// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf definitions
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["../../proto/argus/v1/argus.proto"],
            &["../../proto"],
        )?;

    Ok(())
}
