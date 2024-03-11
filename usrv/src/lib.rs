use std::convert::Infallible;

mod router;

pub use router::{Router, Service};

mod handler;
pub use handler::Handler;

mod from_req;
pub use from_req::{FromRequest, FromRequestRef};

pub struct Request;
impl Request {
    fn matches_path(&self, _path: &str) -> bool {
        todo!()
    }
}

pub struct Response;

impl Response {
    pub fn not_found() -> Self {
        todo!()
    }
}

impl From<()> for Response {
    fn from(_: ()) -> Self {
        todo!()
    }
}

impl From<Infallible> for Response {
    fn from(_value: Infallible) -> Self {
        panic!("Attempt to convert Infallible to Response")
    }
}
