#[macro_use]
extern crate log;

pub use http;

mod error;
pub use error::Error;

mod body;
pub use body::Body;

mod handler;
pub use handler::Handler;

mod from_req;
pub use from_req::{FromRequest, FromRequestRef};

mod response;
pub use response::{IntoResponse, NotFound};

mod router;
pub use router::{Router, Service};

pub type Request = http::Request<Body>;
pub type Response = http::Response<Body>;

mod fill_more;

mod read_req;
pub use read_req::read_request;

mod write_res;
pub use write_res::write_response;

pub mod server;
