use clap::Parser;
use inf_viewer::app::{run, Args};

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("inf-viewer: {error}");
        std::process::exit(1);
    }
}
