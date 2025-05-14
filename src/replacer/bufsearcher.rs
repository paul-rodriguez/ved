use crate::replacer::diff::Diff;
use crate::replacer::error::Result;
use std::collections::VecDeque;
use std::io::Read;

pub const SEARCH_MAX: usize = 4096;

pub struct BufSearcher<'search, R>
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
    pub fn new(pattern: &'search str, replacement: &'search str, reader: &'search mut R) -> Self {
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
