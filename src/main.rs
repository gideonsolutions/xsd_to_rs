use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "xsd_to_rs", about = "Convert XSD files to Rust modules")]
struct Args {
    /// Input XSD file or directory
    #[arg(short, long)]
    input: PathBuf,

    /// Output Rust file or directory
    #[arg(short, long)]
    output: PathBuf,

    /// Nest the generated tree under this module path: emit
    /// `use crate::<prefix>::…` imports and a `mod.rs` root (directory mode only).
    #[arg(long)]
    mod_prefix: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.input.is_dir() {
        xsd_to_rs::directory::convert_directory_with_prefix(
            &args.input,
            &args.output,
            args.mod_prefix.as_deref(),
        )
    } else {
        xsd_to_rs::convert_file(&args.input, &args.output, &[])
    }
}
