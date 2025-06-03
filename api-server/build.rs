use prost_build::Config;
use std::{fs::create_dir, io::Result, path::Path};

fn main() -> Result<()> {
    let out_dir = "generated";

    if !Path::new(out_dir).exists() {
        create_dir(out_dir)?;
    }

    Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .out_dir(out_dir)
        .compile_protos(
            &[
                "../protobufs/meshtastic/crisislab.proto",
            ],
            &["../protobufs"],
        )?;

    Ok(())
}
