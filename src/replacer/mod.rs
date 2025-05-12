mod error;

use error::{Error, Result};
use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::{fs, iter};
use tempfile;

const SEARCH_MAX: usize = 4096;

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
        let diffs = BufSearcher::new(pattern, replacement, &mut input);
        let mut temp_file = tempfile::NamedTempFile::new()?;
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
        let temp_path = temp_file.into_temp_path();
        match fs::rename(temp_path, path) {
            Err(e) => Err(Error::IoError(e)),
            Ok(()) => Ok(()),
        }
    }
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

#[derive(Debug, Eq, PartialEq)]
struct Diff<'str> {
    pos: usize,
    remove: usize,
    add: &'str str,
}

struct BufSearcher<'search, R>
where
    R: std::io::Read,
{
    pattern: &'search str,
    replacement: &'search str,
    pos: usize,
    reader: &'search mut R,
    buf: [u8; SEARCH_MAX],
    read_head: usize,
    drop_head: usize,
    ready: VecDeque<Diff<'search>>,
}

impl<'search, R> BufSearcher<'search, R>
where
    R: std::io::Read,
{
    fn new(pattern: &'search str, replacement: &'search str, reader: &'search mut R) -> Self {
        Self {
            pattern,
            replacement,
            pos: 0,
            reader,
            buf: [0; SEARCH_MAX],
            read_head: 0,
            drop_head: 0,
            ready: VecDeque::new(),
        }
    }

    fn next_diff(self: &mut Self) -> Result<Option<Diff<'search>>> {
        match self.ready.pop_front() {
            Some(d) => Ok(Some(d)),
            None => self.push_diffs(),
        }
    }

    fn push_diffs(self: &mut Self) -> Result<Option<Diff<'search>>> {
        loop {
            self.fill_buffer()?;
            let remaining_bytes = self.read_head - self.drop_head;
            if self.pattern.len() > remaining_bytes {
                // End of file
                break Ok(None);
            }
            match self.match_buffer() {
                None => {
                    self.drop_head += 1;
                }
                Some(diff) => {
                    self.drop_head += self.pattern.len();
                    break Ok(Some(diff));
                }
            };
        }
    }

    fn fill_buffer(self: &mut Self) -> Result<()> {
        let remaining_bytes = self.read_head - self.drop_head;
        if self.pattern.len() > remaining_bytes {
            let missing_bytes = self.pattern.len() - remaining_bytes;
            if self.read_head + missing_bytes >= SEARCH_MAX {
                self.compress_buffer();
            }
            let nb_read = self.reader.read(&mut self.buf[self.read_head..])?;
            self.read_head += nb_read;
        }
        Ok(())
    }

    fn compress_buffer(self: &mut Self) {
        let remaining_bytes = self.read_head - self.drop_head;
        let tmp = self.buf[self.drop_head..self.read_head].to_owned();
        self.buf[..remaining_bytes].clone_from_slice(&tmp);
        self.pos += self.drop_head;
        self.drop_head = 0;
        self.read_head = remaining_bytes;
    }

    fn match_buffer(self: &mut Self) -> Option<Diff<'search>> {
        let slice_end = self.drop_head + self.pattern.len();
        let slice = &self.buf[self.drop_head..slice_end];
        if slice == self.pattern.as_bytes() {
            Some(Diff {
                pos: self.pos + self.drop_head,
                remove: self.pattern.len(),
                add: self.replacement,
            })
        } else {
            None
        }
    }
}

impl<'search, R> Iterator for BufSearcher<'search, R>
where
    R: Read,
{
    type Item = Result<Diff<'search>>;

    fn next(self: &mut Self) -> Option<Result<Diff<'search>>> {
        match self.next_diff() {
            // transpose?
            Ok(None) => None,
            Ok(Some(diff)) => Some(Ok(diff)),
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use io::Cursor;
    use itertools;
    use std::fs;
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
        let pattern: String = iter::repeat("X").take(SEARCH_MAX).collect();
        let orig_content = String::new() + &pattern + " and " + &pattern;
        write_file(&path, &orig_content);
        let result = replace(&pattern, "toto", &path);
        assert!(result.is_ok());

        let content = file_content(path);
        assert_eq!(content, "toto and toto")
    }

    #[test]
    fn test_buf_searcher_basic() {
        let mut input = StringReader::new("abba");
        let mut buf_searcher = BufSearcher::new("abba", "toto", &mut input);
        let option = buf_searcher.next();
        assert!(option.is_some());
        let result = option.unwrap();
        assert!(result.is_ok());
        let diff = result.unwrap();
        let expected = Diff {
            pos: 0,
            remove: 4,
            add: "toto",
        };
        assert_eq!(diff, expected);
    }

    #[test]
    fn test_buf_searcher_two_hits() {
        let mut input = StringReader::new("abba has sold abba records");
        let buf_searcher = BufSearcher::new("abba", "toto", &mut input);
        let diffs: Vec<_> = buf_searcher.map(|x| x.unwrap()).collect();
        let expected = vec![
            Diff {
                pos: 0,
                remove: 4,
                add: "toto",
            },
            Diff {
                pos: 14,
                remove: 4,
                add: "toto",
            },
        ];
        assert_eq!(diffs, expected);
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
