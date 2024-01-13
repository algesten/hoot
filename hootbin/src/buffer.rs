use std::io;
use std::ops::Deref;

const BUFFER_SIZE: usize = 4096;

pub struct InputBuffer<T> {
    inner: Option<T>,
    buffer: [u8; BUFFER_SIZE],
    len: usize,
}

impl<T: io::Read> InputBuffer<T> {
    pub fn new(inner: T) -> Self {
        InputBuffer {
            inner: Some(inner),
            buffer: [0; BUFFER_SIZE],
            len: 0,
        }
    }

    pub fn fill_more(&mut self) -> io::Result<()> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(());
        };

        let (_, unused) = self.buffer.split_at_mut(self.len);
        let amount = inner.read(unused)?;

        if amount == 0 {
            // inner is done reading
            self.inner = None;
        }
        self.len += amount;

        Ok(())
    }

    pub fn is_ended(&self) -> bool {
        self.inner.is_none()
    }

    pub fn consume(&mut self, amount: usize) {
        if amount > self.len {
            panic!("consume more than buffer len");
        }
        self.buffer.copy_within(amount..self.len, 0);
        self.len -= amount;
    }
}

impl<T> Deref for InputBuffer<T> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer[..self.len]
    }
}
