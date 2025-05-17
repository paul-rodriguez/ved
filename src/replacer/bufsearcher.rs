use super::diffheap::DiffHeap;
use crate::replacer::diff::Diff;
use crate::replacer::error::Result;
use std::io::Read;

/// The maximum number of bytes between the start and the end of match.
pub const SEARCH_MAX: usize = 4096;

/// Block patterns will be matched if they exist within the start of a line and this column.
pub const COLUMN_MAX: usize = 120;

pub struct BufSearcher<'search, R>
where
    R: std::io::Read,
{
    patterns: &'search Vec<&'search str>,
    replacement: &'search str,
    pos: usize,
    reader: &'search mut R,
    buf: [u8; SEARCH_MAX],
    read_head: usize,
    drop_head: usize,
    last_line_start: usize,
    ready: DiffHeap<'search>,
}

impl<'search, R> BufSearcher<'search, R>
where
    R: std::io::Read,
{
    pub fn new(
        patterns: &'search Vec<&'search str>,
        replacement: &'search str,
        reader: &'search mut R,
    ) -> Self {
        Self {
            patterns,
            replacement,
            pos: 0,
            reader,
            buf: [0; SEARCH_MAX],
            read_head: 0,
            drop_head: 0,
            last_line_start: 0,
            ready: DiffHeap::new(),
        }
    }

    fn next_diff(self: &mut Self) -> Result<Option<Diff<'search>>> {
        match self.ready.pop() {
            Some(d) => Ok(Some(d)),
            None => {
                let diffs = match self.read_diffs()? {
                    None => return Ok(None),
                    Some(diff_heap) => diff_heap,
                };
                self.ready.merge_with(diffs);
                match self.ready.pop() {
                    None => panic!("Internal error: there should be a diff in the queue, we just added at least one"),
                    Some(d) => Ok(Some(d)),
                }
            }
        }
    }

    fn read_diffs(self: &mut Self) -> Result<Option<DiffHeap<'search>>> {
        loop {
            self.fill_buffer()?;
            let remaining_bytes = self.read_head - self.drop_head;
            if self.minimum_match_length() > remaining_bytes {
                // End of file
                break Ok(None);
            }
            match self.match_buffer() {
                None => {
                    self.drop(1);
                }
                Some(diff_heap) => {
                    self.drop(self.patterns[0].len());
                    break Ok(Some(diff_heap));
                }
            };
        }
    }

    fn drop(self: &mut Self, nb_drop: usize) {
        for _ in 0..nb_drop {
            if self.buf[self.drop_head] == '\n' as u8 {
                self.last_line_start = 0
            } else {
                self.last_line_start += 1
            }
            self.drop_head += 1;
        }
    }

    fn minimum_match_length(self: &Self) -> usize {
        let pattern_sum: usize = self.patterns.iter().map(|p| p.len()).sum();
        let newlines = self.patterns.len() - 1;
        pattern_sum + newlines
    }

    /// Returns the largest number of bytes that a match could span.
    ///
    /// Note that because of vertical matching, there's not really a maximum length, as the match
    /// could start at an arbitrary column.
    /// This the reason why there's a COLUMN_MAX value (this limits the maximum span of a match).
    fn maximum_match_length(self: &Self) -> usize {
        let pattern_sum: usize = self.patterns.iter().map(|p| p.len()).sum();
        let newlines = (self.patterns.len() - 1) * COLUMN_MAX;
        pattern_sum + newlines
    }

    fn fill_buffer(self: &mut Self) -> Result<()> {
        let remaining_bytes = self.read_head - self.drop_head;
        if self.maximum_match_length() > remaining_bytes {
            let missing_bytes = self.maximum_match_length() - remaining_bytes;
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

    /// TODO this really needs a refactor
    fn match_buffer(self: &mut Self) -> Option<DiffHeap<'search>> {
        let mut buf_offset = 0;
        let first_match = self.match_one_pattern(buf_offset, self.patterns[0], self.replacement)?;
        let mut previous_match_len = first_match.diff.remove;
        let line_offset = first_match.line_offset;
        let mut result = DiffHeap::new();
        result.push(first_match.diff);
        for pattern in self.patterns.iter().skip(1) {
            buf_offset += previous_match_len
                + self.next_line_offset(self.drop_head + previous_match_len)?
                + line_offset;
            let mat = self.match_one_pattern(buf_offset, pattern, self.replacement)?;
            previous_match_len = mat.diff.remove;
            result.push(mat.diff);
        }
        Some(result)
    }

    /// Returns the number of bytes between an offset and the next newline.
    ///
    /// What's returned is the offset of the character immediately following the newline character,
    /// not the newline character itself.
    fn next_line_offset(self: &Self, start_offset: usize) -> Option<usize> {
        for i in start_offset..SEARCH_MAX {
            if self.buf[i] == '\n' as u8 {
                return Some(i - start_offset + 1);
            }
        }
        None
    }

    fn match_one_pattern(
        self: &Self,
        offset: usize,
        pattern: &str,
        replacement: &'search str,
    ) -> Option<Match<'search>> {
        let slice_start = self.drop_head + offset;
        let slice_end = self.drop_head + offset + pattern.len();
        let slice = &self.buf[slice_start..slice_end];
        if slice == pattern.as_bytes() {
            Some(Match {
                diff: Diff {
                    pos: self.pos + slice_start,
                    remove: pattern.len(),
                    add: replacement,
                },
                line_offset: self.last_line_start,
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

#[derive(Debug, Eq, PartialEq)]
struct Match<'str> {
    diff: Diff<'str>,
    /// The offset of the diff with the start of the line
    line_offset: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use stringreader::StringReader;

    #[test]
    fn test_basic() {
        let mut input = StringReader::new("abba");
        let patterns = vec!["abba"];
        let mut buf_searcher = BufSearcher::new(&patterns, "toto", &mut input);
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
    fn test_two_hits() {
        let mut input = StringReader::new("abba has sold abba records");
        let patterns = vec!["abba"];
        let buf_searcher = BufSearcher::new(&patterns, "toto", &mut input);
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
    fn test_block_basic() {
        let mut input = StringReader::new("abba\ntoto");
        let patterns = vec!["abba", "toto"];
        let buf_searcher = BufSearcher::new(&patterns, "queen", &mut input);
        let diffs: Vec<_> = buf_searcher.map(|x| x.unwrap()).collect();
        let expected = vec![
            Diff {
                pos: 0,
                remove: 4,
                add: "queen",
            },
            Diff {
                pos: 5,
                remove: 4,
                add: "queen",
            },
        ];
        assert_eq!(diffs, expected);
    }
}
