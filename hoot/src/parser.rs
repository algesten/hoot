use http::{Method, Request, Response, StatusCode, Version};
use httparse::Status;

use crate::Error;

pub fn try_parse_response<const N: usize>(
    input: &[u8],
) -> Result<Option<(usize, Response<()>)>, Error> {
    let mut headers = [httparse::EMPTY_HEADER; N]; // 100 headers ~3kb

    let mut res = httparse::Response::new(&mut headers);

    let maybe_input_used = match res.parse(input) {
        Ok(v) => v,
        Err(e) => {
            return Err(if e == httparse::Error::TooManyHeaders {
                // For expect-100 we use this value to detect that the server
                // sent a regular response instead of a 100-continue.
                Error::HttpParseTooManyHeaders
            } else {
                e.into()
            });
        }
    };

    let input_used = match maybe_input_used {
        Status::Complete(v) => v,
        Status::Partial => return Ok(None),
    };

    let version = {
        let v = res.version.ok_or(Error::MissingResponseVersion)?;
        match v {
            0 => Version::HTTP_10,
            1 => Version::HTTP_11,
            _ => return Err(Error::UnsupportedVersion),
        }
    };

    let status = {
        let v = res.code.ok_or(Error::ResponseMissingStatus)?;
        StatusCode::from_u16(v).map_err(|_| Error::ResponseInvalidStatus)?
    };

    let mut builder = Response::builder().version(version).status(status);

    for h in res.headers {
        builder = builder.header(h.name, h.value);
    }

    let response = builder.body(()).expect("a valid response");

    Ok(Some((input_used, response)))
}

pub fn try_parse_request<const N: usize>(
    input: &[u8],
) -> Result<Option<(usize, Request<()>)>, Error> {
    let mut headers = [httparse::EMPTY_HEADER; N]; // 100 headers ~3kb

    let mut req = httparse::Request::new(&mut headers);

    let maybe_input_used = match req.parse(input) {
        Ok(v) => v,
        Err(e) => {
            return Err(if e == httparse::Error::TooManyHeaders {
                // For expect-100 we use this value to detect that the server
                // sent a regular response instead of a 100-continue.
                Error::HttpParseTooManyHeaders
            } else {
                e.into()
            });
        }
    };

    let input_used = match maybe_input_used {
        Status::Complete(v) => v,
        Status::Partial => return Ok(None),
    };

    let version = {
        let v = req.version.ok_or(Error::MissingResponseVersion)?;
        match v {
            0 => Version::HTTP_10,
            1 => Version::HTTP_11,
            _ => return Err(Error::UnsupportedVersion),
        }
    };

    let method = {
        let v = req.method.ok_or(Error::RequestMissingMethod)?;
        Method::from_bytes(v.as_bytes()).map_err(|_| Error::RequestInvalidMethod)?
    };

    let mut builder = Request::builder().version(version).method(method);

    for h in req.headers {
        builder = builder.header(h.name, h.value);
    }

    let request = builder.body(()).expect("a valid response");

    Ok(Some((input_used, request)))
}

#[cfg(test)]
mod test {
    use crate::parser::try_parse_response;

    #[test]
    fn ensure_no_half_response() {
        let bytes = "HTTP/1.1 200 OK\r\n\
            Content-Type: text/plain\r\n\
            Content-Length: 100\r\n\r\n";

        try_parse_response::<0>(bytes.as_bytes()).expect_err("too many headers");
    }
}
