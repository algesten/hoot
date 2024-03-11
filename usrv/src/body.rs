use std::io::Read;

use crate::response::IntoResponse;

pub enum Body {
    Empty,
    Fixed(Vec<u8>),
    Streaming(Box<dyn Read + Send + 'static>),
}

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Self::Empty
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Self::Fixed(value)
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Self::Fixed(value.to_vec())
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Self::Fixed(value.into_bytes())
    }
}

impl From<&str> for Body {
    fn from(value: &str) -> Self {
        Self::Fixed(value.as_bytes().to_vec())
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
