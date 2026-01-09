mod bufsearcher;
mod diff;
mod diffheap;
mod error;

use crate::teereader;
use bufsearcher::BufSearcher;
use diff::Diff;
use error::{Error, Result};
use glob;
use rand::Rng;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

// TODO change Vec to slice
pub fn replace_glob<'search>(
    patterns: &'search Vec<&'search str>,
    replacements: &'search Vec<&'search str>,
    file_glob: &'search str,
) -> Result<Vec<Result<PathBuf>>> {
    let paths = glob::glob(file_glob)?;

    let results = thread::scope(|scope| {
        let handles: Vec<_> = paths
            .map(|glob_path| {
                scope.spawn(|| {
                    let path = match glob_path {
                        Ok(p) => p,
                        Err(e) => return Err(e.into()),
                    };
                    if !path.as_path().is_dir() {
                        match replace_path(patterns, replacements, &path) {
                            Ok(_) => Ok(path),
                            Err(e) => Err(e),
                        }
                    } else {
                        Ok(path)
                    }
                })
            })
            .collect();
        handles.into_iter().map(|handle| handle.join()?).collect()
    });
    Ok(results)
}

// Search and replace a pattern in a file or recursively in a directory.
//
// For each file that must change, the result of the replacement is first
// written into a temporary file and the original file is replaced by the
// temporary file through a rename.
pub fn replace_path<'search, 'p>(
    patterns: &'search Vec<&'search str>,
    replacements: &'search Vec<&'search str>,
    path: &'p Path,
) -> Result<&'p Path> {
    if path.is_dir() {
        for entry in fs::read_dir(&path)? {
            let entry_path = entry?.path();
            replace_path(patterns, replacements, entry_path.as_path())?;
        }
        Ok(path)
    } else {
        let input = File::open(&path)?;
        let temp_path = temporary_path(&path)?;
        let temp_file = File::create_new(&temp_path)?;
        replace_stream(patterns, replacements, input, temp_file)?;
        match fs::rename(temp_path, &path) {
            Err(e) => Err(Error::IoError(e)),
            Ok(()) => Ok(path),
        }
    }
}

pub fn replace_stream<'s, R, W>(
    patterns: &'s Vec<&'s str>,
    replacements: &'s Vec<&'s str>,
    input: R,
    mut output: W,
) -> Result<()>
where
    R: Read,
    W: Write,
{
    let (mut input1, mut input2) = teereader::tee(input);
    let diffs = BufSearcher::new(patterns, replacements, &mut input1);
    let mut replacer = Replacer::new(Box::new(diffs), &mut input2, &mut output);
    Ok(loop {
        match replacer.replace_next_diff() {
            Err(Error::EndOfIteration) => break,
            Err(e) => return Err(e),
            Ok(()) => (),
        }
    })
}

pub fn replace_single<'s, 'p>(
    pattern: &'s str,
    replacement: &'s str,
    path: &'p Path,
) -> Result<&'p Path> {
    let patterns = vec![pattern];
    let replacements = vec![replacement];
    let result = replace_path(&patterns, &replacements, path);
    return result;
}

fn temporary_path(original_path: &Path) -> Result<PathBuf> {
    let rng = rand::rng();
    let suffix: String = rng
        .sample_iter(rand::distr::Alphanumeric)
        .take(8)
        .map(|c: u8| -> char { c.into() })
        .collect();
    let original_str = match original_path.to_str() {
        None => return Err(Error::PathError(format!("{original_path:?}"))),
        Some(s) => s,
    };
    let pathbuf = PathBuf::from(format!("{original_str}._ved_temp_{suffix}"));
    Ok(pathbuf)
}

struct Replacer<'search, 'iterator, R, W>
where
    R: Read,
    W: Write,
    'search: 'iterator,
{
    diffs: Box<dyn Iterator<Item = Result<Diff<'search>>> + 'iterator>,
    original: &'search mut R,
    output: &'search mut W,
    pos: usize,
}

impl<'search, 'iterator, R, W> Replacer<'search, 'iterator, R, W>
where
    R: Read,
    W: Write,
    'search: 'iterator,
{
    fn new(
        diffs: Box<dyn Iterator<Item = Result<Diff<'search>>> + 'iterator>,
        original: &'search mut R,
        output: &'search mut W,
    ) -> Self {
        Self {
            diffs,
            original,
            output,
            pos: 0,
        }
    }

    fn replace_next_diff(self: &mut Self) -> Result<()> {
        match self.diffs.next() {
            None => {
                self.copy_remaining()?;
                Err(Error::EndOfIteration)
            }
            Some(Err(e)) => return Err(e),
            Some(Ok(diff)) => {
                self.copy_from_original(diff.pos - self.pos)?;
                self.produce_replacement(diff)?;
                Ok(())
            }
        }
    }

    fn copy_remaining(self: &mut Self) -> Result<()> {
        io::copy(self.original, self.output)?;
        Ok(())
    }

    fn produce_replacement(self: &mut Self, diff: Diff) -> Result<()> {
        // skip over the length of the pattern in the input
        let mut buf = vec![0; diff.remove];
        self.original.read_exact(buf.as_mut_slice())?;
        self.output.write_all(diff.add.as_bytes())?;
        self.pos += diff.remove;
        Ok(())
    }

    fn copy_from_original(self: &mut Self, nb_bytes: usize) -> Result<()> {
        let mut buf = vec![0; nb_bytes];
        self.original.read_exact(buf.as_mut_slice())?;
        self.output.write_all(buf.as_slice())?;
        self.pos += nb_bytes;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use super::*;
    use io::Cursor;
    use std::fs;
    use std::iter;
    use stringreader::StringReader;
    use test::Bencher;

    #[test]
    fn test_replace_file_does_not_exist() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        let result = replace_single("abba", "toto", &path);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::IoError(_) => {}
            _ => {
                assert!(false)
            }
        }
    }

    #[test]
    fn test_replace_file_empty() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        let file = File::create_new(&path);
        assert!(file.is_ok());
        let result = replace_single("abba", "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "")
    }

    #[test]
    fn test_replace_basic() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        write_file(&path, "abba");
        let result = replace_single("abba", "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto")
    }

    #[test]
    fn test_replace_two_hits() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        write_file(&path, "abba has sold more records than abba");
        let result = replace_single("abba", "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto has sold more records than toto")
    }

    #[test]
    fn test_replace_max_pattern_len() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        let pattern: String = iter::repeat("X").take(bufsearcher::SEARCH_MAX).collect();
        let orig_content = String::new() + &pattern + " and " + &pattern;
        write_file(&path, &orig_content);
        let result = replace_single(&pattern, "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto and toto")
    }

    #[test]
    fn test_replacer_basic() {
        let mut original = StringReader::new("abba");
        let mut output = Cursor::new(Vec::new());
        let diff = Diff {
            pos: 0,
            remove: 4,
            add: "toto",
        };
        let diffs = iter::once(Ok(diff));
        {
            let mut replacer = Replacer::new(Box::new(diffs), &mut original, &mut output);
            let result = replacer.replace_next_diff();
            assert!(result.is_ok());
        }
        let result = String::from_utf8(output.into_inner());
        assert!(result.is_ok());
        let content = result.unwrap();
        assert_eq!(content, "toto");
    }

    #[test]
    fn test_replace_in_dir() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        write_file(&path, "abba");
        let result = replace_single("abba", "toto", dir.path());
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto")
    }

    #[test]
    fn test_replace_glob() {
        let dir = temp_dir();
        let child_dir = dir.path().join("child");
        assert!(fs::create_dir(&child_dir).is_ok());
        let file1 = child_dir.join("file1");
        write_file(&file1, "hello file1!");
        let file2 = child_dir.join("file2");
        write_file(&file2, "hello file2!");
        let file3 = dir.path().join("file3");
        write_file(&file3, "hello file3!");

        let file_glob = dir.path().as_os_str().to_str().unwrap().to_owned() + "/**/*";
        let paths: Vec<_> = glob::glob(&file_glob).unwrap().collect();
        print!("{paths:?}");

        let result = replace_glob(&vec!["hello"], &vec!["goodbye"], &file_glob);
        assert!(result.is_ok());

        let result1 = file_content(file1);
        assert_eq!(result1, "goodbye file1!");
        let result2 = file_content(file2);
        assert_eq!(result2, "goodbye file2!");
        let result3 = file_content(file3);
        assert_eq!(result3, "goodbye file3!");
    }

    fn time_ed(file_ed: &Path) -> Duration {
        let start = Instant::now();
        let child = Command::new("sed")
            .arg("-e")
            .arg("s/X/Y/g")
            .arg("-i")
            .arg(file_ed.as_os_str().to_str().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let output = child.wait_with_output().unwrap();
        let duration = start.elapsed();
        let err_str = String::from_utf8(output.stderr).unwrap();
        println!("err_str: {err_str}");
        assert!(output.status.success());
        duration
    }

    fn time_ved(file_path: &Path) -> Duration {
        let patterns = vec!["X"];
        let replacements = vec!["Y"];
        let start = Instant::now();
        assert!(replace_path(&patterns, &replacements, file_path).is_ok());
        start.elapsed()
    }

    /// Tests whether ved is faster than ed on a small string.
    ///
    /// This test is biased in favor of ved because we waste time starting ed in a different
    /// process whereas we just call a function to make ved do its job.
    #[test]
    fn test_faster_than_ed_small_no_hits() {
        let dir = temp_dir();
        let content: String = iter::repeat("A").take(100).collect();
        let file_ved = dir.path().join("file_ved");
        write_file(&file_ved, &content);
        let file_ed = dir.path().join("file_ed");
        write_file(&file_ed, &content);

        let ved_time = time_ved(&file_ved);
        let ed_time = time_ed(&file_ed);

        println!("ed: {ed_time:#?}, ved: {ved_time:#?}");

        let ved_result = file_content(file_ved);
        let ed_result = file_content(file_ed);
        let ved_len = ved_result.len();
        let ed_len = ed_result.len();
        println!("ed len: {ed_len}\nved len: {ved_len}");

        assert!(ed_result == ved_result);
        assert!(ed_time > ved_time);
    }

    #[bench]
    fn bench_replacer_all_hits(b: &mut Bencher) {
        let input_str: String = iter::repeat("X").take(10000).collect();
        let patterns = vec!["X"];
        let replacements = vec!["Y"];
        b.iter(move || {
            let input = StringReader::new(&input_str);
            let output = Cursor::new(Vec::new());
            replace_stream(&patterns, &replacements, input, output)
        });
    }

    #[bench]
    fn bench_replacer_no_hits(b: &mut Bencher) {
        let input_str: String = iter::repeat("X").take(10000).collect();
        let patterns = vec!["Y"];
        let replacements = vec!["W"];
        b.iter(move || {
            let input = StringReader::new(&input_str);
            let output = Cursor::new(Vec::new());
            replace_stream(&patterns, &replacements, input, output)
        });
    }

    fn parallel_bench(b: &mut Bencher, nb_files: usize) {
        let patterns_x = vec!["X"];
        let patterns_y = vec!["Y"];
        let dir = temp_dir();
        let content: String = iter::repeat("XH").take(1000).collect();
        for i in 0..nb_files {
            let file_path = dir.path().join(format!("file_{i}"));
            write_file(&file_path, &content);
            print!("Done with {i}");
        }
        let file_glob = dir.path().as_os_str().to_str().unwrap().to_owned() + "/**/*";

        b.iter(
            move || match replace_glob(&patterns_x, &patterns_y, &file_glob) {
                Ok(_) => replace_glob(&patterns_y, &patterns_x, &file_glob),
                Err(e) => Err(e),
            },
        );
    }

    #[bench]
    fn bench_replacer_parallel_2(b: &mut Bencher) {
        parallel_bench(b, 2);
    }

    #[bench]
    fn bench_replacer_parallel_4(b: &mut Bencher) {
        parallel_bench(b, 4);
    }

    #[bench]
    fn bench_replacer_parallel_8(b: &mut Bencher) {
        parallel_bench(b, 8);
    }

    #[bench]
    fn bench_replacer_parallel_16(b: &mut Bencher) {
        parallel_bench(b, 16);
    }

    #[bench]
    fn bench_replacer_parallel_32(b: &mut Bencher) {
        parallel_bench(b, 32);
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
