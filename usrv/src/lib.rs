use std::io;

use hoot::HootError;
pub use http;

pub mod body;
use body::{Body, HootBody};

mod handler;
pub use handler::Handler;

mod from_req;
pub use from_req::{FromRequest, FromRequestRef};

use crate::fill_more::FillMoreBuffer;

pub mod response;
pub mod router;

pub type Request = http::Request<Body>;
pub type Response = http::Response<Body>;

mod fill_more;

pub struct Error;

pub fn read_request<Read>(from: Read) -> Result<Request, Error>
where
    Read: io::Read + Send + 'static,
{
    let mut r = hoot::server::Request::new();
    let mut from = FillMoreBuffer::new(from);

    let mut parse_buf = vec![0_u8; 1024];

    let attempt = loop {
        let input = from.fill_more()?;

        if parse_buf.len() < input.len() {
            parse_buf.resize(input.len(), 0);
        }

        let attempt = r.try_read_request(&input, &mut parse_buf)?;

        if !attempt.is_success() {
            continue;
        }

        break attempt;
    };

    // This much of the input buffer is already used up.
    let input_used = attempt.input_used();

    let req: http::Request<()> = attempt.try_into()?;

    // Remove the amount of input that was used up for the request header.
    from.consume(input_used);

    let body = HootBody {
        request: r.proceed(),
        parse_buf,
        buffer: from,
        leftover: vec![],
    };

    let body = Body::streaming(body);

    let (parts, _) = req.into_parts();

    let req = http::Request::from_parts(parts, body);

    Ok(req)
}

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
