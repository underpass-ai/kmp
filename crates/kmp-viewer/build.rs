use std::fs::File;
use std::io;
use std::path::PathBuf;

use flate2::{Compression, write::GzEncoder};

fn main() -> io::Result<()> {
    let source = "ui/vendor/three.min.js";
    println!("cargo:rerun-if-changed={source}");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"))
        .join("three.min.js.gz");
    let mut encoder = GzEncoder::new(File::create(output)?, Compression::best());
    io::copy(&mut File::open(source)?, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}
