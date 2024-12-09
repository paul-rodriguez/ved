mod replacer;

use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    search: String,

    #[arg(short, long)]
    replace: String,

    #[arg(short, long, default_value = ".")]
    path: String,
}

fn main() {
    let args = Args::parse();

    let result = replacer::replace(&args.search, &args.replace, &Path::new(&args.path));
    match result {
        Ok(_) => {}
        Err(e) => {
            println!("cannot replace: {}", e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main() {}
}
