use std::io::Read;

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
