use clap::Parser;

#[derive(Parser)]
#[command(
    name = "wb",
    version,
    about = "Read and write Whiteboard boards: folders of Markdown notes grouped into sections."
)]
struct Cli {}

fn main() {
    Cli::parse();
}
