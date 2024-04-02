use core::fmt;
use std::cell::RefCell;
use std::io::{self, Cursor, Read};
use std::rc::Rc;

use hoot::types::state::RECV_BODY;

use crate::fill_more::FillMoreBuffer;
use crate::response::IntoResponse;

pub struct Body {
    inner: Inner,
}

enum Inner {
    Empty,
    Bytes(Cursor<Vec<u8>>),
    Streaming(Box<dyn Read + 'static>),
    HootBody(Rc<RefCell<HootBody>>),
}

impl From<Inner> for Body {
    fn from(inner: Inner) -> Self {
        Body { inner }
    }
}

impl Body {
    pub fn empty() -> Body {
        Inner::Empty.into()
    }

    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Body {
        Inner::Bytes(Cursor::new(bytes.into())).into()
    }

    pub fn streaming(read: impl Read + Send + 'static) -> Body {
        Inner::Streaming(Box::new(read)).into()
    }

    pub(crate) fn hoot(body: HootBody) -> Body {
        Inner::HootBody(Rc::new(RefCell::new(body))).into()
    }

    pub(crate) fn hoot_clone(&self) -> Body {
        let Inner::HootBody(rc) = &self.inner else {
            unreachable!()
        };
        let clone = Rc::clone(rc);
        Inner::HootBody(clone).into()
    }

    pub(crate) fn hoot_unwrap(self) -> HootBody {
        let Inner::HootBody(rc) = self.inner else {
            unreachable!()
        };
        let refcell = Rc::try_unwrap(rc).expect("single Rc to HootBody");
        refcell.into_inner()
    }

    pub(crate) fn size(&self) -> Option<u64> {
        match &self.inner {
            Inner::Empty => Some(0),
            Inner::Bytes(v) => Some(v.get_ref().len() as u64),
            Inner::Streaming(_) => None,
            Inner::HootBody(_) => None,
        }
    }
}

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Body::empty()
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Body::bytes(value)
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Body::bytes(value)
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Body::bytes(value)
    }
}

impl From<&str> for Body {
    fn from(value: &str) -> Self {
        Body::bytes(value)
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
    pub buffer: FillMoreBuffer<Box<dyn io::Read + 'static>>,
    pub leftover: Vec<u8>,
}

impl HootBody {
    pub(crate) fn into_buffers(self) -> (Vec<u8>, FillMoreBuffer<Box<dyn io::Read + 'static>>) {
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
        match &mut self.inner {
            Inner::Empty => Ok(0),
            Inner::Bytes(v) => v.read(buf),
            Inner::Streaming(v) => v.read(buf),
            Inner::HootBody(v) => {
                let mut borrow = v.borrow_mut();
                borrow.read(buf)
            }
        }
    }
}

impl fmt::Debug for HootBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HootBody").finish()
    }
}
