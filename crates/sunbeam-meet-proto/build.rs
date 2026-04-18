fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = "../../proto";
    let protos = [
        format!("{proto_root}/meet.proto"),
        format!("{proto_root}/agent.proto"),
    ];

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_well_known_types(false)
        .compile_protos(&protos, &[proto_root.to_string()])?;

    for p in &protos {
        println!("cargo:rerun-if-changed={p}");
    }
    Ok(())
}
