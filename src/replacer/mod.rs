mod error;

use error::Result;
use std::collections::VecDeque;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

const SEARCH_MAX: usize = 4096;

pub fn replace(pattern: &str, replacement: &str, path: &Path) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry_path = entry?.path();
            replace(pattern, replacement, &entry_path)?;
        }
    } else {
        let file = File::open(path)?;
        let diffs = file_diffs(pattern, replacement, file)?;
    }
    todo!()
}

struct Diff<'str> {
    pos: usize,
    remove: usize,
    add: &'str str,
}

fn file_diffs<'replacement>(
    pattern: &str,
    replacement: &'replacement str,
    file: File,
) -> Result<Box<dyn Iterator<Item = Diff<'replacement>>>> {
    let mut reader = BufReader::new(file);
    todo!();
}

struct BufSearcher<'pattern, 'replacement, 'reader> {
    pattern: &'pattern str,
    replacement: &'replacement str,
    pos: usize,
    reader: &'reader mut dyn std::io::Read,
    buf: [u8; SEARCH_MAX],
    read_head: usize,
    drop_head: usize,
    ready: VecDeque<Diff<'replacement>>,
}

impl<'pattern, 'replacement, 'reader> BufSearcher<'pattern, 'replacement, 'reader> {
    fn next_diff(self: &mut Self) -> Result<Diff<'replacement>> {
        match self.ready.pop_front() {
            Some(d) => Ok(d),
            None => {
                self.fill_buffer()?;
            }
        }
        todo!()
    }

    fn fill_buffer(self: &mut Self) -> Result<()> {
        let remaining_bytes = self.read_head - self.drop_head;
        let missing_bytes = self.pattern.len() - remaining_bytes;
        if missing_bytes > 0 {
            if self.read_head + missing_bytes >= SEARCH_MAX {
                self.compress_buffer();
            }
            let nb_read = self.reader.read(&mut self.buf[self.drop_head..])?;
            self.read_head = nb_read;
        }
        Ok(())
    }

    fn compress_buffer(self: &mut Self) {
        let remaining_bytes = self.read_head - self.drop_head;
        let tmp = self.buf[self.drop_head..self.read_head].to_owned();
        self.buf[..remaining_bytes].clone_from_slice(&tmp);
    }
}
