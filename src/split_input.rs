use std::str::SplitWhitespace;

pub struct NullSplitter<'a> {
    buffer: &'a [u8],
}

impl<'a> Iterator for NullSplitter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let null_index = self.buffer.iter().position(|&b| b == 0);
        let output = match null_index {
            None => {
                if self.buffer.is_empty() {
                    return None;
                }
                let output = self.buffer;
                self.buffer = &[];
                output
            }
            Some(null_index) => {
                let (output, rest) = self.buffer.split_at(null_index);
                self.buffer = &rest[1.min(self.buffer.len())..];
                output
            }
        };
        output.utf8_chunks().next().map(|c| c.valid())
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

    pub fn chunks(self, chunk_size: usize) -> SplitterChunks<'a> {
        SplitterChunks {
            iter: self,
            chunk_size,
        }
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

pub struct SplitterChunks<'a> {
    chunk_size: usize,
    iter: Splitter<'a>,
}

impl<'a> Iterator for SplitterChunks<'a> {
    type Item = Vec<&'a str>;

    fn next(&mut self) -> Option<Self::Item> {
        let result: Vec<_> = self.iter.by_ref().take(self.chunk_size).collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_splitter() {
        let buffer = b"foo\0bar\0baz\0";
        let result: Vec<_> = Splitter::null(buffer).collect();
        assert_eq!(result, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn null_splitter_no_null() {
        let buffer = b"foo bar baz";
        let result: Vec<_> = Splitter::null(buffer).collect();
        assert_eq!(result, vec!["foo bar baz"]);
    }

    #[test]
    fn whitespace_splitter() {
        let buffer = b"foo bar baz";
        let result: Vec<_> = Splitter::whitespace(buffer).collect();
        assert_eq!(result, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn whitespace_splitter_no_whitespace() {
        let buffer = b"foo\0bar\0baz\0";
        let result: Vec<_> = Splitter::whitespace(buffer).collect();
        assert_eq!(result, vec!["foo\0bar\0baz\0"]);
    }

    #[test]
    fn splitter_empty() {
        let buffer = b"";
        let result = Splitter::null(buffer).collect::<Vec<_>>();
        assert_eq!(result, Vec::<&str>::new());
        let result: Vec<_> = Splitter::whitespace(buffer).collect();
        assert_eq!(result, Vec::<&str>::new());
    }

    #[test]
    fn bad_utf8() {
        let buffer = b"foo\xFFbar";
        let result: Vec<_> = Splitter::null(buffer).collect();
        assert_eq!(result, vec!["foo"]);
        let result: Vec<_> = Splitter::whitespace(buffer).collect();
        assert_eq!(result, vec!["foo"]);
    }

    #[test]
    fn chunks_1() {
        let buffer = b"foo\0bar\0baz\0";
        let result: Vec<_> = Splitter::null(buffer).chunks(1).collect();
        assert_eq!(result, vec![vec!["foo"], vec!["bar"], vec!["baz"]]);
    }

    #[test]
    fn chunks_incomplete() {
        let buffer = b"foo\0bar\0baz\0";
        let result: Vec<_> = Splitter::null(buffer).chunks(2).collect();
        assert_eq!(result, vec![vec!["foo", "bar"], vec!["baz"]]);
    }
}
