use core::fmt::{self, Write};

use crate::{HootError, Result, OVERFLOW};

pub(crate) struct Out<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Out<'a> {
    pub fn wrap(buf: &'a mut [u8]) -> Self {
        Out { buf, pos: 0 }
    }

    pub fn write<'b>(&mut self, bytes: &'b [u8]) -> Result<usize> {
        if bytes.len() >= self.buf.len() {
            return Err(HootError::OutputOverflow);
        }

        self.buf[self.pos..(self.pos + bytes.len())].copy_from_slice(bytes);
        self.pos += bytes.len();

        Ok(bytes.len())
    }

    pub fn borrow_remaining(&mut self) -> &mut [u8] {
        &mut self.buf[self.pos..]
    }

    pub fn flush(self) -> &'a [u8] {
        &self.buf[..self.pos]
    }

    pub fn write_send_line(&mut self, method: &str, path: &str, version: &str) -> Result<()> {
        write!(self, "{} {} HTTP/{}\r\n", method, path, version).or(OVERFLOW)
    }
}

impl<'a> fmt::Write for Out<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write(s.as_bytes()).and(Ok(())).or(Err(fmt::Error))
    }
}
