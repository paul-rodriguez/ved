#![feature(test)]

mod replacer;
mod teereader;

use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    search: String,

    #[arg(short, long)]
    replace: String,

    #[arg(default_value = ".")]
    paths: Vec<String>,
}

fn main() {
    let args = Args::parse();
    run(args)
}

fn run(args: Args) {
    let paths: Vec<&str> = args.paths.iter().map(String::as_str).collect();
    let results = replacer::replace_paths(&vec![&args.search], &vec![&args.replace], &paths);

    for result in results {
        if let Err(e) = result {
            println!("cannot replace: {}", e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_run() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        write_file(&path, "aaaaa");
        run(Args {
            search: "a".to_string(),
            replace: "b".to_string(),
            paths: vec![path.to_str().unwrap().to_owned()],
        });
        let content = file_content(&path);
        assert_eq!(content, "bbbbb");
    }

    fn temp_dir() -> tempfile::TempDir {
        let result = tempfile::tempdir();
        assert!(result.is_ok());
        result.unwrap()
    }

    fn file_content<P: AsRef<Path>>(path: P) -> String {
        let result = fs::read_to_string(path);
        assert!(result.is_ok());
        result.unwrap()
    }

    fn write_file<P: AsRef<Path>>(path: P, content: &str) {
        let result = fs::write(path, content);
        assert!(result.is_ok());
    }
}
