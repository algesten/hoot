mod router;
use std::convert::Infallible;

pub use router::{Router, Service};

mod handler;

mod from_req;

pub struct Request;
impl Request {
    fn matches_path(&self, path: &str) -> bool {
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
