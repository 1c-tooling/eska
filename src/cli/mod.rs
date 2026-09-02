use clap::Parser;

/// Command-line interface for eska.
#[derive(Parser, Debug)]
#[command(name = "eska", about, version)]
pub struct Cli {}
