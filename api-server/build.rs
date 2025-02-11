use std::{
    io::Result,
    fs::create_dir,
    path::Path
};
use prost_build::Config;

fn main() -> Result<()> {
    let out_dir = "generated";

    if !Path::new(out_dir).exists() {
        create_dir(out_dir)?;
    }

    Config::new()
        .type_attribute(".", "#[derive(serde::Serialize)]")
        .out_dir(out_dir)
        .compile_protos(&["../protobufs/meshtastic/mesh.proto"], &["../protobufs"])?;

    Ok(())
}
