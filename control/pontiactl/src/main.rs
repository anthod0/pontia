use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "pontiactl",
    version,
    about = "Control Pontia from the command line"
)]
struct Cli {}

fn main() {
    Cli::parse();
}
