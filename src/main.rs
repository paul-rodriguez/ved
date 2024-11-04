mod replacer;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    substitution: String,

    #[arg(short, long, default_value = ".")]
    path: String,
}

fn main() {
    let args = Args::parse();
}
