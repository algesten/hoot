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

    fn output<'b>(&mut self, bytes: &'b [u8]) -> Result<usize> {
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

    pub fn writer<'b>(&'b mut self) -> Writer<'b, 'a> {
        Writer {
            out: self,
            inc: Some(0),
        }
    }

    // pub fn pos(&self) -> usize {
    //     self.pos
    // }

    // pub fn set_pos(&mut self, pos: usize) {
    //     assert!(pos <= self.pos);
    //     self.pos = pos;
    // }

    // pub fn split_and_borrow_remaining(&mut self, pos: usize) -> (&[u8], &mut [u8]) {
    //     assert!(pos <= self.pos);

    //     // We need the buffer in two parts. Written and unused.
    //     let (used, rest) = self.buf.split_at_mut(self.pos);

    //     (&used[pos..], rest)
    // }

    pub fn flush(self) -> &'a [u8] {
        &self.buf[..self.pos]
    }

    pub fn write_send_line(&mut self, method: &str, path: &str, version: &str) -> Result<()> {
        write!(self.writer(), "{} {} HTTP/{}\r\n", method, path, version).or(OVERFLOW)
    }
}

pub(crate) struct Writer<'b, 'a> {
    out: &'b mut Out<'a>,
    inc: Option<usize>,
}

impl<'b, 'a> Writer<'b, 'a> {
    #[inline(always)]
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<usize> {
        let ret = self.out.output(bytes);

        if let Err(_) = &ret {
            // Do not increase position if we encountered an error while writing.
            self.inc = None;
        } else {
            // Increase position with written amount. If this is None, we have
            // encounterd an error in an earlier write, and do not want to increase
            // the position.
            if let Some(inc) = &mut self.inc {
                *inc += bytes.len();
            }
        }

        ret
    }
}

impl<'b, 'a> fmt::Write for Writer<'b, 'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        self.write_bytes(bytes).and(Ok(())).or(Err(fmt::Error))
    }
}

impl<'b, 'a> Drop for Writer<'b, 'a> {
    fn drop(&mut self) {
        if let Some(inc) = self.inc.take() {
            // Commit increase to borrowed Out.
            self.out.pos += inc;
        }
    }
}
