use core::str;

use crate::error::Result;
use crate::parser::find_crlf;
use crate::HootError;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dechunker {
    Size,
    Chunk(usize),
    CrLf,
    Ended,
}

struct Pos {
    index_in: usize,
    index_out: usize,
}

impl Dechunker {
    pub fn new() -> Self {
        Dechunker::Size
    }

    pub fn parse_input(&mut self, src: &[u8], dst: &mut [u8]) -> Result<(usize, usize)> {
        let mut pos = Pos {
            index_in: 0,
            index_out: 0,
        };

        loop {
            let more = match self {
                Dechunker::Size => self.read_size(src, &mut pos)?,
                Dechunker::Chunk(_) => self.read_data(src, dst, &mut pos)?,
                Dechunker::CrLf => self.read_crlf(src, &mut pos)?,
                Dechunker::Ended => false,
            };

            if !more {
                break;
            }
        }

        Ok((pos.index_in, pos.index_out))
    }

    #[cfg(test)]
    fn left(&self) -> usize {
        if let Self::Chunk(l) = self {
            *l
        } else {
            0
        }
    }

    pub fn is_ended(&self) -> bool {
        matches!(self, Self::Ended)
    }

    fn read_size(&mut self, src: &[u8], pos: &mut Pos) -> Result<bool> {
        let src = &src[pos.index_in..];

        let Some(i) = find_crlf(src) else {
            return Ok(false);
        };

        let len_end = src.iter().position(|c| *c == b';').unwrap_or(i);
        let len_str = str::from_utf8(&src[..len_end])?;
        let len = usize::from_str_radix(len_str, 16)?;

        pos.index_in += i + 2;
        *self = if len == 0 {
            Self::Ended
        } else {
            Self::Chunk(len)
        };

        Ok(true)
    }

    fn read_data(&mut self, src: &[u8], dst: &mut [u8], pos: &mut Pos) -> Result<bool> {
        let src = &src[pos.index_in..];
        let dst = &mut dst[pos.index_out..];

        let Self::Chunk(left) = self else {
            unreachable!();
        };

        // Read the smallest amount of input/output or length left of chunk.
        let to_read = src.len().min(dst.len()).min(*left);

        (&mut dst[..to_read]).copy_from_slice(&src[..to_read]);
        pos.index_in += to_read;
        pos.index_out += to_read;
        *left -= to_read;

        if *left == 0 {
            *self = Self::CrLf;
        }

        Ok(to_read > 0)
    }

    fn read_crlf(&mut self, src: &[u8], pos: &mut Pos) -> Result<bool> {
        let src = &src[pos.index_in..];

        let Some(i) = find_crlf(src) else {
            return Ok(false);
        };

        if i > 0 {
            return Err(HootError::IncorrectChunk);
        }

        pos.index_in += 2;
        *self = Self::Size;

        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_dechunk_size() -> Result<()> {
        let mut d = Dechunker::new();
        let mut b = [0; 1024];
        assert_eq!(d.parse_input(b"", &mut b)?, (0, 0));
        assert_eq!(d.parse_input(b"2", &mut b)?, (0, 0));
        assert_eq!(d.parse_input(b"2\r", &mut b)?, (0, 0));
        assert_eq!(d.left(), 0);
        assert_eq!(d.parse_input(b"2\r\n", &mut b)?, (3, 0));
        assert_eq!(d.left(), 2);
        Ok(())
    }

    #[test]
    fn test_dechunk_size_meta() -> Result<()> {
        let mut d = Dechunker::new();
        let mut b = [0; 1024];
        assert_eq!(d.parse_input(b"2;meta\r", &mut b)?, (0, 0));
        assert_eq!(d.parse_input(b"2;meta\r\n", &mut b)?, (8, 0));
        Ok(())
    }

    #[test]
    fn test_dechunk_data() -> Result<()> {
        let mut d = Dechunker::new();
        let mut b = [0; 1024];
        assert_eq!(d.parse_input(b"2\r\nOK", &mut b)?, (5, 2));
        assert_eq!(&b[..2], b"OK");
        assert_eq!(d.left(), 0);
        assert_eq!(d.parse_input(b"\r\n", &mut b)?, (2, 0));
        assert_eq!(d.left(), 0);
        assert!(!d.is_ended());
        assert_eq!(d.parse_input(b"0\r\n", &mut b)?, (3, 0));
        assert!(d.is_ended());
        Ok(())
    }
}
