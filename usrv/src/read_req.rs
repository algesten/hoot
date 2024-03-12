use std::io;

use crate::body::{Body, HootBody};
use crate::fill_more::FillMoreBuffer;
use crate::{Error, Request};

pub fn read_request<Read>(reader: Read) -> Result<Option<Request>, Error>
where
    Read: io::Read + 'static,
{
    let parse_buf = vec![0_u8; 1024];

    let boxed: Box<dyn io::Read + 'static> = Box::new(reader);
    let fill_buf = FillMoreBuffer::new(boxed);

    read_from_buffers(parse_buf, fill_buf)
}

pub(crate) fn read_from_buffers(
    mut parse_buf: Vec<u8>,
    mut fill_buf: FillMoreBuffer<Box<dyn io::Read + 'static>>,
) -> Result<Option<Request>, Error> {
    let mut hoot_req = hoot::server::Request::new();

    let attempt = loop {
        let input = fill_buf.fill_more()?;

        if parse_buf.len() < input.len() {
            parse_buf.resize(input.len(), 0);
        }

        let attempt = hoot_req.try_read_request(&input, &mut parse_buf)?;

        if !attempt.is_success() {
            continue;
        }

        break attempt;
    };

    // This much of the input buffer is already used up.
    let input_used = attempt.input_used();

    let req: http::Request<()> = attempt.try_into()?;

    // Remove the amount of input that was used up for the request header.
    fill_buf.consume(input_used);

    let body = HootBody {
        hoot_req: hoot_req.proceed(),
        parse_buf,
        buffer: fill_buf,
        leftover: vec![],
    };

    let body = Body::internal(body);

    let (parts, _) = req.into_parts();

    let req = http::Request::from_parts(parts, body);

    Ok(Some(req))
}
