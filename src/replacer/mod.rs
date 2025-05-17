mod bufsearcher;
mod diff;
mod diffheap;
mod error;

use bufsearcher::BufSearcher;
use diff::Diff;
use error::{Error, Result};
use rand::Rng;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

// Search and replace a pattern in a file or recursively in a directory.
//
// For each file that must change, the result of the replacement is first
// written into a temporary file and the original file is replaced by the
// temporary file through a rename.
pub fn replace(pattern: &str, replacement: &str, path: &Path) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry_path = entry?.path();
            replace(pattern, replacement, &entry_path)?;
        }
        Ok(())
    } else {
        let mut input = File::open(path)?;
        let patterns = vec![pattern];
        let replacements = vec![replacement];
        let diffs = BufSearcher::new(&patterns, &replacements, &mut input);
        let temp_path = temporary_path(path)?;
        let mut temp_file = File::create_new(&temp_path)?;
        let mut original = File::open(path)?;
        {
            let mut replacer = Replacer::new(Box::new(diffs), &mut original, &mut temp_file);
            loop {
                match replacer.replace_next_diff() {
                    Err(Error::EndOfIteration) => break,
                    Err(e) => return Err(e),
                    Ok(()) => (),
                }
            }
        }
        match fs::rename(temp_path, path) {
            Err(e) => Err(Error::IoError(e)),
            Ok(()) => Ok(()),
        }
    }
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
    use super::*;
    use io::Cursor;
    use std::fs;
    use std::iter;
    use stringreader::StringReader;

    #[test]
    fn test_replace_file_does_not_exist() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        let result = replace("abba", "toto", &path);
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
        let result = replace("abba", "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "")
    }

    #[test]
    fn test_replace_basic() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        write_file(&path, "abba");
        let result = replace("abba", "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto")
    }

    #[test]
    fn test_replace_two_hits() {
        let dir = temp_dir();
        let path = dir.path().join("file");
        write_file(&path, "abba has sold more records than abba");
        let result = replace("abba", "toto", &path);
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
        let result = replace(&pattern, "toto", &path);
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
        let result = replace("abba", "toto", dir.path());
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto")
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
