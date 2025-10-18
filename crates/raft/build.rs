fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile our transport.proto with latest tonic/prost
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/transport.proto"], &["proto"])?;

    Ok(())
}
