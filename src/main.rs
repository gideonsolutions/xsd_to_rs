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
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.input.is_dir() {
        xsd_to_rs::directory::convert_directory(&args.input, &args.output)
    } else {
        xsd_to_rs::convert_file(&args.input, &args.output, &[])
    }
}
