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

    fn output<'b>(&mut self, bytes: &'b [u8], from: usize) -> Result<usize> {
        let start = self.pos + from;
        let remaining = self.buf.len() - start;
        let len = bytes.len();

        if len > remaining {
            return Err(HootError::OutputOverflow);
        }

        let into = &mut self.buf[start..(start + len)];
        into.copy_from_slice(bytes);

        Ok(len)
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

    pub fn flush(self) -> &'a [u8] {
        &self.buf[..self.pos]
    }

    pub fn write_send_line(&mut self, method: &str, path: &str, version: &str) -> Result<()> {
        let mut w = self.writer();
        write!(w, "{} {} HTTP/{}\r\n", method, path, version).or(OVERFLOW)?;
        w.commit();
        Ok(())
    }
}

pub(crate) struct Writer<'b, 'a> {
    out: &'b mut Out<'a>,
    inc: Option<usize>,
}

impl<'b, 'a> Writer<'b, 'a> {
    #[inline(always)]
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<usize> {
        let ret = self.out.output(bytes, self.inc.unwrap_or(0));

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

    // Splits the buffer in Out at the position: Out.pos + Writer.inc.
    // The first half is chopped off to where Writer started.
    //
    // [o o o o o o o o w w w w w w w w w w w e e e e e e e e]
    //
    // o = old data (i.e. when we called Out::writer())
    // w = data written by this writer
    // e = empty space after the writer
    //
    // This function returns
    // (&[w], &mut [e])
    pub fn split_and_borrow(&mut self) -> (&[u8], &mut [u8]) {
        let s = self.out.pos;
        let Some(i) = self.inc else {
            return (&[], &mut []);
        };
        let e = s + i;

        // We need the buffer in two parts. Written and unused.
        let (used, rest) = self.out.buf.split_at_mut(e);

        (&used[s..e], rest)
    }

    pub fn commit(mut self) {
        if let Some(inc) = self.inc.take() {
            // Commit increase to borrowed Out.
            self.out.pos += inc;
        }
    }
}

impl<'b, 'a> fmt::Write for Writer<'b, 'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        self.write_bytes(bytes).and(Ok(())).or(Err(fmt::Error))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn write_and_commit() {
        let mut buf = [0; 1024];
        let mut out = Out::wrap(&mut buf);
        assert_eq!(out.pos, 0);

        let mut w = out.writer();
        write!(w, "testing 123{}", "456").unwrap();
        assert_eq!(w.inc, Some(14));

        w.commit();
        assert_eq!(out.pos, 14);

        assert_eq!(std::str::from_utf8(&buf[0..14]).unwrap(), "testing 123456");
    }
}
