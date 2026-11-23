use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

fn append_rust_sources(directory: &Path, output: &mut File) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        let file_type = fs::symlink_metadata(&path)?.file_type();
        if file_type.is_dir() {
            append_rust_sources(&path, output)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
            println!("cargo:rerun-if-changed={}", path.display());
            output.write_all(&fs::read(path)?)?;
            output.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("CARGO_MANIFEST_DIR is missing"))?;
    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("OUT_DIR is missing"))?;
    let mut output = File::create(out_dir.join("policy-sources.rs"))?;
    for directory in [manifest_dir.join("src"), manifest_dir.join("tests")] {
        if directory.is_dir() {
            append_rust_sources(&directory, &mut output)?;
        }
    }
    Ok(())
}
