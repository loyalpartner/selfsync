use std::path::{Path, PathBuf};

use heck::ToUpperCamelCase;
use prost_types::FileDescriptorSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = PathBuf::from("proto");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    let proto_files: Vec<PathBuf> = std::fs::read_dir(&proto_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();

    let mut config = prost_build::Config::new();
    let fds = config.load_fds(&proto_files, &[&proto_dir])?;
    generate_data_type_id_table(&fds, &out_dir)?;
    config.compile_fds(fds)?;

    Ok(())
}

/// Walk the FileDescriptorSet, find the `EntitySpecifics.specifics_variant`
/// oneof, and emit a Rust `match` mapping each variant to its proto field
/// number — which is, by Chromium's convention, the data type id.
fn generate_data_type_id_table(
    fds: &FileDescriptorSet,
    out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut variants: Vec<(String, i32)> = Vec::new();

    'outer: for file in &fds.file {
        for msg in &file.message_type {
            if msg.name() != "EntitySpecifics" {
                continue;
            }
            let Some(oneof_idx) = msg
                .oneof_decl
                .iter()
                .position(|o| o.name() == "specifics_variant")
            else {
                continue;
            };
            let oneof_idx = oneof_idx as i32;
            for field in &msg.field {
                if field.oneof_index == Some(oneof_idx) {
                    variants.push((field.name().to_upper_camel_case(), field.number()));
                }
            }
            break 'outer;
        }
    }

    if variants.is_empty() {
        return Err("EntitySpecifics.specifics_variant oneof not found in descriptors".into());
    }

    let mut out = String::from(
        "// @generated from EntitySpecifics.specifics_variant by build.rs — do not edit.\n\n",
    );
    out.push_str(
        "/// Returns the wire-format tag of the populated `specifics_variant`,\n\
         /// which doubles as Chromium's data type id; 0 if the oneof is unset.\n",
    );
    out.push_str("pub fn extract_data_type_id(entry: &sync_pb::SyncEntity) -> i32 {\n");
    out.push_str("    use sync_pb::entity_specifics::SpecificsVariant;\n");
    out.push_str("    let Some(specifics) = entry.specifics.as_ref() else { return 0; };\n");
    out.push_str(
        "    let Some(variant) = specifics.specifics_variant.as_ref() else { return 0; };\n",
    );
    out.push_str("    match variant {\n");
    for (name, tag) in &variants {
        out.push_str(&format!("        SpecificsVariant::{name}(_) => {tag},\n"));
    }
    out.push_str("    }\n");
    out.push_str("}\n");

    std::fs::write(out_dir.join("data_type_id.rs"), out)?;
    Ok(())
}
