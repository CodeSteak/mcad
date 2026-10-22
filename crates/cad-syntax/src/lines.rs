//! Conversions between byte offsets (Rust), UTF-16 offsets (CodeMirror / JS) and
//! line/column positions.

#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of each line start.
    line_starts: Vec<u32>,
    /// For each char boundary with a multi-unit difference: (byte offset, utf16 offset).
    /// Empty for ASCII-only sources.
    checkpoints: Vec<(u32, u32)>,
    len_bytes: u32,
    ascii: bool,
}

impl LineIndex {
    pub fn new(src: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        let ascii = src.is_ascii();
        let mut checkpoints = Vec::new();
        if !ascii {
            let mut u16pos = 0u32;
            for (i, ch) in src.char_indices() {
                if ch.len_utf8() != ch.len_utf16() {
                    checkpoints.push((i as u32, u16pos));
                }
                u16pos += ch.len_utf16() as u32;
                if ch.len_utf8() != ch.len_utf16() {
                    checkpoints.push(((i + ch.len_utf8()) as u32, u16pos));
                }
            }
        }
        LineIndex { line_starts, checkpoints, len_bytes: src.len() as u32, ascii }
    }

    pub fn byte_to_utf16(&self, byte: u32) -> u32 {
        if self.ascii {
            return byte;
        }
        // Last checkpoint at or before `byte`.
        match self.checkpoints.partition_point(|&(b, _)| b <= byte) {
            0 => byte,
            n => {
                let (b, u) = self.checkpoints[n - 1];
                u + (byte - b)
            }
        }
    }

    pub fn utf16_to_byte(&self, u16pos: u32) -> u32 {
        if self.ascii {
            return u16pos.min(self.len_bytes);
        }
        match self.checkpoints.partition_point(|&(_, u)| u <= u16pos) {
            0 => u16pos,
            n => {
                let (b, u) = self.checkpoints[n - 1];
                (b + (u16pos - u)).min(self.len_bytes)
            }
        }
    }

    /// Zero-based line of a byte offset.
    pub fn line_of(&self, byte: u32) -> u32 {
        (self.line_starts.partition_point(|&s| s <= byte) - 1) as u32
    }

    /// Zero-based (line, byte column).
    pub fn line_col(&self, byte: u32) -> (u32, u32) {
        let l = self.line_of(byte);
        (l, byte - self.line_starts[l as usize])
    }

    pub fn line_start(&self, line: u32) -> u32 {
        self.line_starts.get(line as usize).copied().unwrap_or(self.len_bytes)
    }

    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }
}
