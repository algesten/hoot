use std::cell::RefCell;
use std::io::{self, Cursor, Read};
use std::mem;
use std::rc::Rc;

use hoot::types::state::RECV_BODY;

use crate::fill_more::FillMoreBuffer;
use crate::response::IntoResponse;

#[non_exhaustive]
pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    Streaming(Box<dyn Read + 'static>),

    #[doc(hidden)]
    #[allow(private_interfaces)]
    Internal(InternalBody),

    #[doc(hidden)]
    Cursor(Cursor<Vec<u8>>),
}

#[derive(Clone)]
pub(crate) struct InternalBody(pub Rc<RefCell<HootBody>>);

impl InternalBody {
    pub fn into_inner(self) -> HootBody {
        let cell = Rc::into_inner(self.0).expect("single reference to InternalBody");
        cell.into_inner()
    }
}

impl Body {
    pub fn empty() -> Body {
        Body::Empty
    }

    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Body {
        Body::Bytes(bytes.into())
    }

    pub fn streaming(read: impl Read + 'static) -> Body {
        Body::Streaming(Box::new(read))
    }

    pub(crate) fn internal(body: HootBody) -> Body {
        Body::Internal(InternalBody(Rc::new(RefCell::new(body))))
    }

    pub(crate) fn size(&self) -> Option<usize> {
        match self {
            Body::Empty => Some(0),
            Body::Bytes(v) => Some(v.len()),
            Body::Streaming(_) => None,
            Body::Internal(_) => None,
            Body::Cursor(v) => Some(v.get_ref().len()),
        }
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

pub(crate) struct HootBody {
    pub hoot_req: hoot::server::Request<RECV_BODY>,
    pub parse_buf: Vec<u8>,
    pub buffer: FillMoreBuffer<Box<dyn Read + 'static>>,
    pub leftover: Vec<u8>,
}

impl HootBody {
    pub(crate) fn into_buffers(self) -> (Vec<u8>, FillMoreBuffer<Box<dyn Read + 'static>>) {
        assert!(self.leftover.is_empty());
        (self.parse_buf, self.buffer)
    }
}

impl io::Read for HootBody {
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
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

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

impl io::Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Body::Empty => Ok(0),
            Body::Bytes(v) => {
                let bytes = mem::take(v);
                *self = Body::Cursor(Cursor::new(bytes));
                self.read(buf)
            }
            Body::Streaming(v) => v.read(buf),
            Body::Internal(v) => {
                let mut borrow = RefCell::borrow_mut(&v.0);
                borrow.read(buf)
            }
            Body::Cursor(v) => v.read(buf),
        }
    }
}
