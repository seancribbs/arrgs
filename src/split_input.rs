use std::str::SplitWhitespace;

pub(crate) struct NullSplitter<'a> {
    buffer: &'a [u8],
}

impl<'a> Iterator for NullSplitter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let mut output: &[u8] = &self.buffer[0..0];
        for i in 0..self.buffer.len() {
            output = &self.buffer[..i];
            if self.buffer[i] == 0 {
                self.buffer = &self.buffer[i + 1..];
                break;
            }
        }
        Some(output.utf8_chunks().next()?.valid())
    }
}

pub enum Splitter<'a> {
    Null(NullSplitter<'a>),
    Whitespace(SplitWhitespace<'a>),
}

impl<'a> Splitter<'a> {
    pub fn null(buffer: &'a [u8]) -> Self {
        Self::Null(NullSplitter { buffer })
    }

    pub fn whitespace(buffer: &'a [u8]) -> Self {
        let contents = buffer.utf8_chunks().next().map_or("", |c| c.valid());
        Self::Whitespace(contents.split_whitespace())
    }
}

impl<'a> Iterator for Splitter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Splitter::Null(null_splitter) => null_splitter.next(),
            Splitter::Whitespace(split_whitespace) => split_whitespace.next(),
        }
    }
}
