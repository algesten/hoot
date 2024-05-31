use std::io::{self, Cursor};

pub(crate) fn find_crlf(b: &[u8]) -> Option<usize> {
    let cr = b.iter().position(|c| *c == b'\r')?;
    let maybe_lf = b.get(cr + 1)?;
    if *maybe_lf == b'\n' {
        Some(cr)
    } else {
        None
    }
}

pub(crate) fn compare_lowercase_ascii(a: &str, lowercased: &str) -> bool {
    if a.len() != lowercased.len() {
        return false;
    }

    for (a, b) in a.chars().zip(lowercased.chars()) {
        if !a.is_ascii() {
            return false;
        }
        let norm = a.to_ascii_lowercase();
        if norm != b {
            return false;
        }
    }

    true
}

pub(crate) struct Writer<'a>(pub Cursor<&'a mut [u8]>);

impl<'a> Writer<'a> {
    pub(crate) fn new(output: &'a mut [u8]) -> Writer<'a> {
        Self(Cursor::new(output))
    }

    pub fn len(&self) -> usize {
        self.0.position() as usize
    }

    pub fn available(&self) -> usize {
        self.0.get_ref().len() - self.len()
    }

    pub(crate) fn try_write(&mut self, block: impl Fn(&mut Self) -> io::Result<()>) -> bool {
        let pos = self.0.position();
        let success = (block)(self).is_ok();
        if !success {
            self.0.set_position(pos);
        }
        success
    }
}

impl<'a> io::Write for Writer<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
