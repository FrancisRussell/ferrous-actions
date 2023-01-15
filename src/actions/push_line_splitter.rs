use std::borrow::Cow;
use std::collections::VecDeque;

/// A platform-agnostic line splitter.
///
/// This is part of a work-around for
/// <https://github.com/FrancisRussell/ferrous-actions-dev/issues/81>. It treats any group of CR and
/// LF characters with no repeated characters as a line break.
///
/// `close()` must be called when the input source has hit EOF so final lines
/// may be returned.

#[derive(Debug, Clone, Default)]
pub struct PushLineSplitter {
    lines: VecDeque<(usize, usize)>,
    taken: usize,
    buffer: Vec<u8>,
    scan_offset: usize,
    scan_flags: u8,
    line_len: usize,
    delim_len: usize,
    closed: bool,
}

pub struct WriteBuffer<'a> {
    length: usize,
    parent: &'a mut PushLineSplitter,
}

impl<'a> AsMut<[u8]> for WriteBuffer<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        let buffer_len = self.parent.buffer.len();
        &mut self.parent.buffer[(buffer_len - self.length)..]
    }
}

impl Drop for WriteBuffer<'_> {
    fn drop(&mut self) {
        self.parent.post_write();
    }
}

impl PushLineSplitter {
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    fn pre_write(&mut self) {
        assert!(!self.closed, "Data written after close");
        self.drain_taken();
    }

    fn post_write(&mut self) {
        self.update_scan();
    }

    pub fn write(&mut self, data: &[u8]) {
        self.pre_write();
        self.buffer.extend(data);
        self.post_write();
    }

    pub fn write_via_buffer(&mut self, len: usize) -> WriteBuffer {
        self.pre_write();
        let buffer_len = self.buffer.len();
        self.buffer.resize(buffer_len + len, 0u8);
        WriteBuffer {
            length: len,
            parent: self,
        }
    }

    pub fn close(&mut self) {
        if !self.closed {
            self.closed = true;
            // The current line is finished, whether or not it is delimiter terminated
            self.lines.push_back((self.line_len, self.delim_len));
            // If we are mid-way through a delimiter, we also need an empty line
            if self.scan_flags != 0 {
                self.lines.push_back((0, 0));
            }
        }
    }

    pub fn next_line(&mut self) -> Option<Cow<str>> {
        if let Some((line_len, delim_len)) = self.lines.pop_front() {
            let slice = &self.buffer[self.taken..(self.taken + line_len)];
            let line = String::from_utf8_lossy(slice);
            self.taken += line_len + delim_len;
            Some(line)
        } else {
            None
        }
    }

    fn drain_taken(&mut self) {
        if self.taken > 0 {
            drop(self.buffer.drain(..self.taken));
            self.scan_offset -= self.taken;
            self.taken = 0;
        }
    }

    fn update_scan(&mut self) {
        while self.scan_offset < self.buffer.len() {
            let c = self.buffer[self.scan_offset];
            let c_flags = Self::delimiter_flags(c);
            // We terminate the delimiter because of a new non-NL character or repeated
            // newline
            if (self.scan_flags != 0 && c_flags == 0) || (c_flags & self.scan_flags) != 0 {
                self.scan_flags = 0;
                self.lines.push_back((self.line_len, self.delim_len));
                (self.line_len, self.delim_len) = (0, 0);
            }
            if c_flags == 0 {
                self.line_len += 1;
            } else {
                self.delim_len += 1;
                self.scan_flags |= c_flags;
            }
            self.scan_offset += 1;
        }
    }

    fn delimiter_flags(character: u8) -> u8 {
        const LF: u8 = 10;
        const CR: u8 = 13;
        match character {
            LF => 1,
            CR => 2,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn test_inputs() -> Vec<Vec<String>> {
        let test_vectors: [&[&str]; 6] = [
            &[""],
            &["the quick", "brown fox"],
            &["", "the quick", "brown fox", "jumped over", ""],
            &["", "", "", "", ""],
            &["a", "", "", "b", "", "", "c"],
            &["", "a", "", "b", "", "c", ""],
        ];
        let mut result = Vec::with_capacity(test_vectors.len());
        for lines in test_vectors {
            result.push(lines.into_iter().copied().map(String::from).collect());
        }
        result
    }

    #[derive(Copy, Clone, Debug)]
    enum Mode {
        AllAtOnce,
        Bytes,
    }

    fn test_reconstruction(mode: Mode) {
        for delimiter in ["\n", "\r", "\r\n"] {
            for input in test_inputs().into_iter() {
                let string = input.join(delimiter);
                let bytes = string.as_bytes();
                let mut splitter = PushLineSplitter::default();
                let mut lines = Vec::with_capacity(input.len());
                match mode {
                    Mode::AllAtOnce => {
                        splitter.write(bytes);
                        splitter.close();
                        while let Some(line) = splitter.next_line() {
                            lines.push(line.into_owned());
                        }
                    }
                    Mode::Bytes => {
                        for byte in bytes.iter().copied() {
                            let byte = [byte];
                            splitter.write(&byte[..]);
                            while let Some(line) = splitter.next_line() {
                                lines.push(line.into_owned());
                            }
                        }
                        splitter.close();
                        while let Some(line) = splitter.next_line() {
                            lines.push(line.into_owned());
                        }
                    }
                }
                let reconstructed = lines.join(delimiter);
                assert_eq!(string, reconstructed);
            }
        }
    }

    #[wasm_bindgen_test]
    fn bulk_write() {
        test_reconstruction(Mode::AllAtOnce);
    }

    #[wasm_bindgen_test]
    fn byte_at_a_time_write() {
        test_reconstruction(Mode::Bytes);
    }
}
