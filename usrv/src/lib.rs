use std::convert::Infallible;

pub use http;

pub mod router;

mod handler;
pub use handler::Handler;

mod from_req;
pub use from_req::{FromRequest, FromRequestRef};

mod response;

pub type Request = http::Request<Body>;
pub type Response = http::Response<Body>;

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub enum Body {
    Empty,
}

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Body::Empty
    }
}

impl IntoResponse for Infallible {
    fn into_response(self) -> Response {
        panic!("IntoResponse for Infallible");
    }
}

impl IntoResponse for () {
    fn into_response(self) -> Response {
        http::Response::builder().body(Body::Empty).unwrap()
    }
}
