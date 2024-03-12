use std::io;

const INCREMENT: usize = 4096;
const THRESHOLD: usize = 100;

pub struct FillMoreBuffer<Read> {
    buffer: Vec<u8>,
    pos: usize,
    reader: Option<Read>,
}

impl<Read: io::Read> FillMoreBuffer<Read> {
    pub fn new(reader: Read) -> Self {
        Self {
            buffer: vec![0; INCREMENT],
            pos: 0,
            reader: Some(reader),
        }
    }

    pub fn fill_more(&mut self) -> io::Result<&[u8]> {
        let Some(reader) = &mut self.reader else {
            return Ok(self.buffer());
        };

        if self.pos > self.buffer.len() - THRESHOLD {
            self.buffer.resize(self.buffer.len() + INCREMENT, 0);
        }

        let n = reader.read(&mut self.buffer[self.pos..])?;
        self.pos += n;

        if n == 0 {
            // Free readers as soon as possible.
            self.reader = None;
        }

        Ok(self.buffer())
    }

    pub fn consume(&mut self, amount: usize) {
        let max = amount.min(self.pos);
        self.buffer.copy_within(max.., 0);
        self.pos -= max;
    }

    fn buffer(&self) -> &[u8] {
        &self.buffer[..self.pos]
    }
}
