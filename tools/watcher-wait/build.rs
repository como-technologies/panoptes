// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile argus v1 proto for GetWatchState RPC (client only)
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["../../proto/argus/v1/argus.proto"], &["../../proto"])?;

    Ok(())
}
