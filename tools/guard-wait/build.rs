// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile janus v1 proto for GetGuardState RPC (client only)
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["../../proto/janus/v1/janus.proto"], &["../../proto"])?;

    Ok(())
}
