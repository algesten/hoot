use std::io::{self, Read};

use hoot::types::state::RECV_BODY;

use crate::fill_more::FillMoreBuffer;
use crate::response::IntoResponse;

pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    Streaming(Box<dyn Read + Send + 'static>),
}

impl Body {
    pub fn empty() -> Body {
        Body::Empty
    }

    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Body {
        Body::Bytes(bytes.into())
    }

    pub fn streaming(read: impl Read + Send + 'static) -> Body {
        Body::Streaming(Box::new(read))
    }
}

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Self::Empty
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Self::Bytes(value.to_vec())
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Self::Bytes(value.into_bytes())
    }
}

impl From<&str> for Body {
    fn from(value: &str) -> Self {
        Self::Bytes(value.as_bytes().to_vec())
    }
}

impl<T> IntoResponse for T
where
    T: Into<Body>,
{
    fn into_response(self) -> crate::Response {
        http::Response::new(self.into())
    }
}

pub(crate) struct HootBody<Read> {
    pub hoot_req: hoot::server::Request<RECV_BODY>,
    pub parse_buf: Vec<u8>,
    pub buffer: FillMoreBuffer<Read>,
    pub leftover: Vec<u8>,
}

impl<Read: io::Read> io::Read for HootBody<Read> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.leftover.is_empty() {
            let max = self.leftover.len().min(buf.len());
            buf[..max].copy_from_slice(&self.leftover[..max]);
            self.leftover.drain(..max);
            return Ok(max);
        }

        let input = self.buffer.fill_more()?;

        if input.is_empty() {
            return Ok(0);
        }

        if self.parse_buf.len() < input.len() {
            self.parse_buf.resize(input.len(), 0);
        }

        let part = self
            .hoot_req
            .read_body(input, &mut self.parse_buf)
            .expect("TODO");

        let input_used = part.input_used();

        let data = part.data();

        let max = buf.len().min(data.len());
        buf[..max].copy_from_slice(&data[..max]);

        if data.len() > max {
            self.leftover.extend_from_slice(&data[max..]);
        }

        self.buffer.consume(input_used);

        Ok(max)
    }
}
