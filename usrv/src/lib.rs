use std::io;

use hoot::HootError;
pub use http;

pub mod body;
use body::Body;

mod handler;
pub use handler::Handler;

mod from_req;
pub use from_req::{FromRequest, FromRequestRef};

pub mod response;
pub mod router;

pub type Request = http::Request<Body>;
pub type Response = http::Response<Body>;

mod fill_more;

mod read_req;
pub use read_req::read_request;

mod write_res;
pub use write_res::write_response;

pub struct Error;

impl From<HootError> for Error {
    fn from(value: hoot::HootError) -> Self {
        todo!()
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        todo!()
    }
}
