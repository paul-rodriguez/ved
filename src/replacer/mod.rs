mod error;

use error::Result;
use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::{fs, iter};

const SEARCH_MAX: usize = 4096;

pub fn replace(pattern: &str, replacement: &str, path: &Path) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry_path = entry?.path();
            replace(pattern, replacement, &entry_path)?;
        }
    } else {
        let diffs = stream_diffs(pattern, replacement, Box::new(File::open(path)?));
    }
    todo!()
}

fn replace_diffs<'replacement, 'search>(
    mut diffs: Box<dyn Iterator<Item = Result<Diff<'replacement>>> + 'search>,
    original: Box<dyn Read>,
    output: Box<dyn Write>,
) -> Result<()> {
    let mut replacer = Replacer::new(diffs, original, output);
    loop {
        replacer.replace_next_diff()?;
    }
    Ok(())
}

struct Replacer<'replacement, 'search> {
    diffs: Box<dyn Iterator<Item = Result<Diff<'replacement>>> + 'search>,
    original: Box<dyn Read>,
    output: Box<dyn Write>,
    pos: usize,
}

impl<'replacement, 'search> Replacer<'replacement, 'search> {
    fn new(
        mut diffs: Box<dyn Iterator<Item = Result<Diff<'replacement>>> + 'search>,
        original: Box<dyn Read>,
        output: Box<dyn Write>,
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
                Ok(())
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
        io::copy(self.original.as_mut(), self.output.as_mut())?;
        Ok(())
    }

    fn produce_replacement(self: &mut Self, diff: Diff) -> Result<()> {
        // skip over the length of the pattern in the input
        let mut buf = vec![0; diff.remove];
        self.original.read_exact(buf.as_mut_slice())?;
        self.output.write_all(diff.add.as_bytes());
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

struct Diff<'str> {
    pos: usize,
    remove: usize,
    add: &'str str,
}

fn stream_diffs<'search, 'pattern, 'replacement>(
    pattern: &'pattern str,
    replacement: &'replacement str,
    stream: Box<dyn Read>,
) -> Box<dyn Iterator<Item = Result<Diff<'replacement>>> + 'search>
where
    'search: 'pattern + 'replacement,
    'pattern: 'search,
    'replacement: 'search,
{
    let reader = Box::new(BufReader::new(stream));
    let mut searcher = BufSearcher::new(pattern, replacement, reader);
    let iterator = iter::from_fn(move || match searcher.next_diff() {
        Ok(None) => None,
        Ok(Some(diff)) => Some(Ok(diff)),
        Err(e) => Some(Err(e)),
    });
    Box::new(iterator)
}

struct BufSearcher<'search> {
    pattern: &'search str,
    replacement: &'search str,
    pos: usize,
    reader: Box<dyn std::io::Read>,
    buf: [u8; SEARCH_MAX],
    read_head: usize,
    drop_head: usize,
    ready: VecDeque<Diff<'search>>,
}

impl<'search> BufSearcher<'search> {
    fn new(
        pattern: &'search str,
        replacement: &'search str,
        reader: Box<dyn std::io::Read>,
    ) -> Self {
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
            let missing_bytes = self.pattern.len() - remaining_bytes;
            if missing_bytes > 0 {
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
        let missing_bytes = self.pattern.len() - remaining_bytes;
        if missing_bytes > 0 {
            if self.read_head + missing_bytes >= SEARCH_MAX {
                self.compress_buffer();
            }
            let nb_read = self.reader.read(&mut self.buf[self.drop_head..])?;
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
        if &self.buf[self.drop_head..] == self.pattern.as_bytes() {
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
