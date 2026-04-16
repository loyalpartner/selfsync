use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = PathBuf::from("proto");

    let proto_files: Vec<PathBuf> = std::fs::read_dir(&proto_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();

    prost_build::Config::new().compile_protos(&proto_files, &[&proto_dir])?;

    Ok(())
}
