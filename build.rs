use std::error::Error;
use std::fmt::Write;
use std::path::{Component, PathBuf};
use std::{env, fs};
use walkdir::WalkDir;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=src/assets");

    let mut output_macros = String::from("{\n");
    for entry in WalkDir::new("src/assets") {
        let entry = entry?;
        if entry.file_type().is_dir() {
            continue;
        }

        let embedded_path = entry
            .path()
            .components()
            .skip_while(|x| x != &Component::Normal("assets".as_ref()))
            .fold("msp_map_editor".to_string(), |mut s, c| {
                s.push('/');
                s.push_str(&c.as_os_str().to_string_lossy());
                s
            });

        let full_path = fs::canonicalize(entry.path())?;
        let full_path = full_path.to_string_lossy();

        if let Some(base_path) = embedded_path.strip_suffix(".meta") {
            writeln!(
                output_macros,
                "  embedded_meta!({base_path:?}, {full_path:?});"
            )?;
        } else {
            writeln!(
                output_macros,
                "  embedded_asset!({embedded_path:?}, {full_path:?});"
            )?;
        }
    }
    output_macros.push('}');
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("asset_index.rs"),
        output_macros,
    )?;

    Ok(())
}
