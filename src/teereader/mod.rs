// This file was AI-generated originally, be careful

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

/// Splits a single Reader into two independent Readers.
/// Data read from the source is buffered until both readers have consumed it.
pub fn tee<R: Read>(source: R) -> (impl Read + Seek, impl Read + Seek) {
    let shared = Arc::new(Mutex::new(Shared {
        reader: source,
        buffer: VecDeque::new(),
        global_offset: 0,
        pos: [0, 0],
        active: [true, true],
    }));

    (
        TeeReader {
            id: 0,
            shared: shared.clone(),
        },
        TeeReader { id: 1, shared },
    )
}

// --- Internal Details ---

struct Shared<R> {
    reader: R,
    buffer: VecDeque<u8>,
    global_offset: usize, // The absolute position of the start of the buffer
    pos: [usize; 2],      // The absolute position of each reader
    active: [bool; 2],    // Tracks if a reader has been dropped
}

struct TeeReader<R> {
    id: usize,
    shared: Arc<Mutex<Shared<R>>>,
}

impl<R: Read> Read for TeeReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut state = self.shared.lock().unwrap();

        // 1. Determine where we are relative to the buffer
        let my_pos = state.pos[self.id];
        let start_pos = state.global_offset;
        let buffer_len = state.buffer.len();

        // Calculate index in the VecDeque
        // Note: my_pos is always >= start_pos because we truncate based on min(pos)
        let relative_idx = my_pos - start_pos;

        // 2. If we have data buffered, read from it
        if relative_idx < buffer_len {
            // How much is available in the buffer for us?
            let available = buffer_len - relative_idx;
            // How much can we actually copy to the user's buf?
            let to_read = std::cmp::min(buf.len(), available);

            // Copy slice is tricky with VecDeque, so we iterate or use slices
            // (VecDeque::as_slices is efficient here)
            let (front, back) = state.buffer.as_slices();

            // Logic to copy from the correct offset in the ring buffer
            // Simple approach: Copy byte-by-byte or use a helper.
            // For brevity/correctness here, we use a loop or flattening.
            // Optimized approach:
            let src_iter = front
                .iter()
                .chain(back.iter())
                .skip(relative_idx)
                .take(to_read);

            let buf_iter = buf.iter_mut();
            for t in src_iter.zip(buf_iter) {
                let (src_byte, dst_byte) = t;
                *dst_byte = *src_byte;
            }

            state.pos[self.id] += to_read;
            self.cleanup(&mut state);
            return Ok(to_read);
        }

        // 3. If we are caught up (no buffer left for us), read from source
        // We read directly into the user's buffer for zero-copy,
        // THEN push that data into our internal backup buffer for the other reader.
        let n = state.reader.read(buf)?;

        if n > 0 {
            // Save what we just read for the sibling reader
            state.buffer.extend(&buf[..n]);
            state.pos[self.id] += n;
        }

        self.cleanup(&mut state);
        Ok(n)
    }
}

impl<R: Read> Seek for TeeReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let mut state = self.shared.lock().unwrap();

        match pos {
            SeekFrom::Current(n) if n >= 0 => {
                let mut remaining = n as usize;

                // 1. Advance through the data we already have in the buffer
                let my_pos = state.pos[self.id];
                let buffer_len = state.buffer.len();
                let relative_idx = my_pos - state.global_offset;

                if relative_idx < buffer_len {
                    let in_buffer = std::cmp::min(remaining, buffer_len - relative_idx);
                    state.pos[self.id] += in_buffer;
                    remaining -= in_buffer;
                }

                // 2. If we still need to seek forward, read from source and buffer it
                if remaining > 0 {
                    // We use a temporary stack buffer to perform the "skip"
                    let mut skip_buf = [0u8; 8192];
                    while remaining > 0 {
                        let to_read = std::cmp::min(remaining, skip_buf.len());
                        // Use the existing read logic to ensure data is buffered for the sibling
                        // We call the Read implementation's logic directly via the shared state
                        let n = state.reader.read(&mut skip_buf[..to_read])?;
                        if n == 0 {
                            break;
                        } // EOF reached

                        state.buffer.extend(&skip_buf[..n]);
                        state.pos[self.id] += n;
                        remaining -= n;
                    }
                }

                self.cleanup(&mut state);
                Ok(state.pos[self.id] as u64)
            }
            SeekFrom::Current(n) if n < 0 => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Backward seek is not supported by TeeReader",
            )),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Only relative forward seek is supported by TeeReader",
            )),
        }
    }
}

impl<R> TeeReader<R> {
    // Drops data from the buffer that both readers have already seen
    fn cleanup(&self, state: &mut Shared<R>) {
        // Find the minimum position among ACTIVE readers
        let min_pos = if state.active[0] && state.active[1] {
            std::cmp::min(state.pos[0], state.pos[1])
        } else if state.active[0] {
            state.pos[0]
        } else if state.active[1] {
            state.pos[1]
        } else {
            state.global_offset + state.buffer.len() // Both dead
        };

        let remove_count = min_pos.saturating_sub(state.global_offset);
        if remove_count > 0 {
            state.buffer.drain(0..remove_count);
            state.global_offset += remove_count;
        }
    }
}

// Ensure we handle the case where one reader is dropped early
impl<R> Drop for TeeReader<R> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.lock() {
            state.active[self.id] = false;
            // Trigger cleanup immediately so we don't hold memory for a dead reader
            self.cleanup(&mut state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    #[test]
    fn test_basic_duplication() {
        let data = b"Hello, world!";
        let source = Cursor::new(data);
        let (mut r1, mut r2) = tee(source);

        let mut out1 = Vec::new();
        let mut out2 = Vec::new();

        r1.read_to_end(&mut out1).unwrap();
        r2.read_to_end(&mut out2).unwrap();

        assert_eq!(out1, data);
        assert_eq!(out2, data);
    }

    #[test]
    fn test_interleaved_reads() {
        // Test reading small chunks alternately to stress the internal buffer offsets
        let data = b"0123456789";
        let source = Cursor::new(data);
        let (mut r1, mut r2) = tee(source);

        let mut buf = [0u8; 2];

        // R1 reads "01"
        assert_eq!(r1.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf, b"01");

        // R2 reads "01" (from buffer)
        assert_eq!(r2.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf, b"01");

        // R1 reads "23"
        assert_eq!(r1.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf, b"23");

        // R1 reads "45" (R2 is now behind by 4 bytes)
        assert_eq!(r1.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf, b"45");

        // R2 reads "23" (catch up)
        assert_eq!(r2.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf, b"23");

        // Finish up
        let mut rest1 = String::new();
        let mut rest2 = String::new();
        r1.read_to_string(&mut rest1).unwrap();
        r2.read_to_string(&mut rest2).unwrap();

        assert_eq!(rest1, "6789");
        assert_eq!(rest2, "456789");
    }

    #[test]
    fn test_drop_one_reader_early() {
        // Ensures that if one reader dies, the other can still finish
        // and internal buffers don't grow infinitely for the dead reader.
        let data = b"Long string of data to ensure we pass buffer limits if any";
        let source = Cursor::new(data);
        let (mut r1, r2) = tee(source);

        // Reader 1 reads a little bit
        let mut buf = [0u8; 5];
        r1.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"Long ");

        // Drop Reader 2 explicitly
        drop(r2);

        // Reader 1 should be able to continue reading the rest
        let mut result = String::new();
        r1.read_to_string(&mut result).unwrap();

        assert_eq!(
            result,
            "string of data to ensure we pass buffer limits if any"
        );
    }

    #[test]
    fn test_different_buffer_sizes() {
        // One reader uses a tiny buffer, the other uses a huge one
        let data = [1u8; 1024]; // 1KB of ones
        let source = Cursor::new(&data[..]);
        let (mut r1, mut r2) = tee(source);

        let mut out1 = Vec::new();
        let mut out2 = Vec::new();

        // R1 reads byte-by-byte
        for _ in 0..1024 {
            let mut tiny_buf = [0u8; 1];
            r1.read_exact(&mut tiny_buf).unwrap();
            out1.push(tiny_buf[0]);
        }

        // R2 reads all at once
        r2.read_to_end(&mut out2).unwrap();

        assert_eq!(out1, data);
        assert_eq!(out2, data);
    }

    #[test]
    fn test_empty_source() {
        let source = Cursor::new(b"");
        let (mut r1, mut r2) = tee(source);

        let mut buf = Vec::new();
        assert_eq!(r1.read_to_end(&mut buf).unwrap(), 0);
        assert_eq!(r2.read_to_end(&mut buf).unwrap(), 0);
    }
}
