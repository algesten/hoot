use crate::model::{HttpVersion, Status};
use crate::util::cast_buf_for_headers;
use crate::vars::private;
use crate::version::HTTP_10;
use crate::Result;
use crate::{Call, HootError};

use crate::state::*;
use private::*;

pub enum ParseResult<'a, 'b, S1: State, V: Version, M: Method, B: BodyType, S2: State> {
    Incomplete(Call<'a, S1, V, M, B>),
    Complete(Call<'a, S2, V, M, B>, usize, Status<'b>),
}

impl<'a, V: Version, M: Method, B: BodyType> Call<'a, RECV_STATUS, V, M, B> {
    pub fn parse_status<'b>(
        mut self,
        buf: &'b [u8],
    ) -> Result<ParseResult<'a, 'b, RECV_STATUS, V, M, B, RECV_HEADERS>> {
        let mut response = {
            // Borrow the remaining byte buffer temporarily for header parsing.
            let tmp = self.out.borrow_remaining();
            let headers = cast_buf_for_headers(tmp)?;
            httparse::Response::new(headers)
        };

        response.parse(buf)?;

        // HTTP/1.0 200 OK
        let (Some(version), Some(code)) = (response.version, response.code) else {
            return Ok(ParseResult::Incomplete(self));
        };

        if version != V::httparse_version() {
            return Err(HootError::InvalidHttpVersion);
        }

        let status = Status(HttpVersion::Http10, code, response.reason);

        Ok(ParseResult::Complete(self.transition(), 0, status))
    }
}

// https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
// Messages MUST NOT include both a Content-Length header field and a
// non-identity transfer-coding. If the message does include a non-
// identity transfer-coding, the Content-Length MUST be ignored.
