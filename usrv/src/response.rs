use std::convert::Infallible;

use http::HeaderValue;

use crate::body::ContentType;
use crate::{Body, Response};

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub struct NotFound;

impl IntoResponse for NotFound {
    fn into_response(self) -> Response {
        http::Response::builder()
            .status(404)
            .body(Body::empty())
            .unwrap()
    }
}

impl IntoResponse for Infallible {
    fn into_response(self) -> Response {
        panic!("IntoResponse for Infallible");
    }
}

impl<T> IntoResponse for T
where
    T: Into<Body>,
{
    fn into_response(self) -> crate::Response {
        let body: Body = self.into();

        let ctype = body
            .ctype
            .unwrap_or(ContentType("application/octet-stream"));

        let mut res = http::Response::new(body);

        res.headers_mut()
            .append("content-type", HeaderValue::from_static(ctype.0));

        if let Some(size) = res.body().size() {
            res.headers_mut().append("content-length", size.into());
        }

        res
    }
}
