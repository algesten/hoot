use core::marker::PhantomData;
use core::mem;
use core::str;

use httparse::Header;

use crate::parser::{find_crlf, parse_headers, parse_response_line};
use crate::req::CallState;
use crate::util::{compare_lowercase_ascii, LengthChecker};
use crate::vars::private::*;
use crate::{state::*, HootError, HttpVersion};
use crate::{Result, ResumeToken};

pub struct Response<S: State> {
    _typ: PhantomData<S>,
    state: CallState,
}

impl Response<()> {
    pub fn resume(request: ResumeToken<ENDED, (), (), ()>) -> Response<RECV_RESPONSE> {
        Response {
            _typ: PhantomData,
            state: request.into_state(),
        }
    }

    #[cfg(test)]
    fn new_test() -> Response<RECV_RESPONSE> {
        Response {
            _typ: PhantomData,
            state: CallState {
                version: Some(HttpVersion::Http11),
                is_head: Some(false),
                ..Default::default()
            },
        }
    }
}

impl<S: State> Response<S> {
    fn transition<S2: State>(self) -> Response<S2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }
}

pub struct ResponseAttempt<'a, 'b> {
    response: Response<RECV_RESPONSE>,
    success: bool,
    status: Option<Status<'a>>,
    headers: Option<&'b [Header<'a>]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Status<'a>(pub HttpVersion, pub u16, pub &'a str);

impl<'a, 'b> ResponseAttempt<'a, 'b> {
    fn incomplete(response: Response<RECV_RESPONSE>) -> Self {
        ResponseAttempt {
            response,
            success: false,
            status: None,
            headers: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn status(&self) -> Option<&Status<'a>> {
        self.status.as_ref()
    }

    pub fn headers(&self) -> Option<&'b [Header<'a>]> {
        self.headers
    }

    pub fn next(self) -> AttemptNext {
        if !self.success {
            AttemptNext::Retry(self.response.transition())
        } else {
            match self.response.state.recv_body_mode.unwrap() {
                RecvBodyMode::LengthDelimited(v) if v == 0 => {
                    AttemptNext::NoBody(self.response.transition())
                }
                _ => AttemptNext::Body(self.response.transition()),
            }
        }
    }
}

pub enum AttemptNext {
    Retry(Response<RECV_RESPONSE>),
    Body(Response<RECV_BODY>),
    NoBody(Response<ENDED>),
}

impl AttemptNext {
    pub fn unwrap_retry(self) -> Response<RECV_RESPONSE> {
        let AttemptNext::Retry(r) = self else {
            panic!("unwrap_no_body when AttemptNext isnt Retry");
        };
        r
    }

    pub fn unwrap_body(self) -> Response<RECV_BODY> {
        let AttemptNext::Body(r) = self else {
            panic!("unwrap_no_body when AttemptNext isnt Body");
        };
        r
    }

    pub fn unwrap_no_body(self) -> Response<ENDED> {
        let AttemptNext::NoBody(r) = self else {
            panic!("unwrap_no_body when AttemptNext isnt NoBody");
        };
        r
    }
}

impl Response<RECV_RESPONSE> {
    pub fn try_read_response<'a, 'b>(
        mut self,
        input: &'a [u8],
        buf: &'b mut [u8],
    ) -> Result<ResponseAttempt<'a, 'b>> {
        let line = parse_response_line(input)?;
        if !line.success {
            return Ok(ResponseAttempt::incomplete(self));
        }

        // Unwrap is ok because we must have set the version earlier in request.
        let request_version = self.state.version.unwrap();

        if request_version != line.output.0 {
            return Err(HootError::HttpVersionMismatch);
        }

        let status_offset = line.consumed;

        let headers = parse_headers(&input[status_offset..], buf)?;
        if !headers.success {
            return Ok(ResponseAttempt::incomplete(self));
        }

        // Derive body mode from knowledge this far.
        let is_http10 = request_version == HttpVersion::Http10;
        let is_head = self.state.is_head.unwrap(); // Ok for same reason as above.
        let mode = RecvBodyMode::from(is_http10, is_head, line.output.1, headers.output)?;

        // If we are awaiting a length, put a length checker in place
        if let RecvBodyMode::LengthDelimited(len) = mode {
            if len > 0 {
                self.state.recv_checker = Some(LengthChecker::new(len));
            }
        }

        // Remember te body mode
        self.state.recv_body_mode = Some(mode);

        Ok(ResponseAttempt {
            response: self,
            success: true,
            status: Some(line.output),
            headers: Some(headers.output),
        })
    }
}

impl Response<RECV_BODY> {
    pub fn read_body<'a, 'b>(
        &mut self,
        src: &'a [u8],
        dst: &'b mut [u8],
    ) -> Result<ReadResult<'b>> {
        // If we already read to completion, do not use any more input.
        if self.state.body_complete {
            return Ok(ReadResult {
                input_used: 0,
                output: &[],
                is_complete: true,
            });
        }

        // unwrap is ok because we can't be in state RECV_BODY without setting it.
        let ret = match self.state.recv_body_mode.unwrap() {
            RecvBodyMode::LengthDelimited(_) => self.read_limit(src, dst, true),
            RecvBodyMode::Chunked => read_chunked(src, dst),
            RecvBodyMode::CloseDelimited => self.read_limit(src, dst, false),
        };

        let is_complete = ret.as_ref().map(|r| r.is_complete).unwrap_or(false);
        if is_complete {
            self.state.body_complete = true;
        }

        ret
    }

    fn read_limit<'a, 'b>(
        &mut self,
        src: &'a [u8],
        dst: &'b mut [u8],
        use_checker: bool,
    ) -> Result<ReadResult<'b>> {
        let input_used = src.len().min(dst.len());

        let mut is_complete = false;
        if use_checker {
            let checker = self.state.recv_checker.as_mut().unwrap();
            checker.append(input_used, HootError::RecvMoreThanContentLength)?;
            is_complete = checker.is_complete();
        }

        let output = &mut dst[..input_used];
        output.copy_from_slice(&src[..input_used]);
        Ok(ReadResult {
            input_used,
            output,
            is_complete,
        })
    }
}

fn read_chunked<'a, 'b>(src: &'a [u8], dst: &'b mut [u8]) -> Result<ReadResult<'b>> {
    let mut index_in = 0;
    let mut index_out = 0;
    let mut is_complete = false;

    while let Some((len_in, len_out)) = read_chunk(&src[index_in..], &mut dst[index_out..])? {
        if len_in == 0 {
            is_complete = true;
            break;
        }

        index_in += len_in;
        index_out += len_out;
    }

    Ok(ReadResult {
        input_used: index_in,
        output: &dst[..index_out],
        is_complete,
    })
}

fn read_chunk(src: &[u8], dst: &mut [u8]) -> Result<Option<(usize, usize)>> {
    let Some(crlf1) = find_crlf(src) else {
        return Ok(None);
    };
    let chunk_len = &src[..crlf1];

    let len_end = chunk_len.iter().position(|c| *c == b';').unwrap_or(crlf1);
    let len_str = str::from_utf8(&src[..len_end])?;
    let len = usize::from_str_radix(len_str, 16)?;

    // We read an entire chunk at a time.
    let required_input = crlf1 + 2 + len + 2;

    // Check there is enough length in input and output for the entire chunk.
    if src.len() < required_input || dst.len() < len {
        return Ok(None);
    }

    // Double check the input ends \r\n.
    if src[required_input - 2] != b'\r' || src[required_input - 1] != b'\n' {
        return Err(HootError::IncorrectChunk);
    }

    let from = {
        let x = &src[crlf1 + 2..];
        &x[..len]
    };

    (&mut dst[..len]).copy_from_slice(from);

    Ok(Some((required_input, len)))
}

pub struct ReadResult<'b> {
    pub input_used: usize,
    pub output: &'b [u8],
    pub is_complete: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RecvBodyMode {
    /// Delimited by content-length. 0 is also a valid value when we don't expect a body,
    /// due to HEAD or status, but still want to leave the socket open.
    LengthDelimited(u64),
    /// Chunked transfer encoding
    Chunked,
    /// Expect remote to close at end of body.
    CloseDelimited,
}

impl RecvBodyMode {
    pub fn from(
        is_http10: bool,
        is_head: bool,
        status_code: u16,
        headers: &[Header<'_>],
    ) -> Result<Self> {
        let has_no_body =
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
            // All responses to the HEAD request method
            // MUST NOT include a message-body, even though the presence of entity-
            // header fields might lead one to believe they do.
            is_head ||
            // All 1xx (informational), 204 (no content), and 304 (not modified) responses
            // MUST NOT include a message-body.
            status_code >= 100 && status_code <= 199 ||
            matches!(status_code, 204 | 304);

        if has_no_body {
            return Ok(Self::LengthDelimited(0));
        }

        // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
        // All other responses do include a message-body, although it MAY be of zero length.

        let mut content_length: Option<u64> = None;
        let mut is_chunked = false;

        for head in headers {
            if compare_lowercase_ascii(head.name, "content-length") {
                let v = str::from_utf8(head.value)?.parse::<u64>()?;
                if content_length.is_some() {
                    return Err(HootError::DuplicateContentLength);
                }
                content_length = Some(v);
            } else if !is_chunked && compare_lowercase_ascii(head.name, "transfer-encoding") {
                // Header can repeat, stop looking if we found "chunked"
                let s = str::from_utf8(head.value)?;
                is_chunked = s
                    .split(",")
                    .map(|v| v.trim())
                    .any(|v| compare_lowercase_ascii(v, "chunked"));
            }
        }

        if is_chunked && !is_http10 {
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
            // Messages MUST NOT include both a Content-Length header field and a
            // non-identity transfer-coding. If the message does include a non-
            // identity transfer-coding, the Content-Length MUST be ignored.
            return Ok(Self::Chunked);
        }

        if let Some(len) = content_length {
            return Ok(Self::LengthDelimited(len));
        }

        Ok(Self::CloseDelimited)
    }
}

#[cfg(any(std, test))]
mod std_impls {
    use super::*;
    use std::fmt;

    impl fmt::Debug for Status<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("Status")
                .field(&self.0)
                .field(&self.1)
                .field(&self.2)
                .finish()
        }
    }

    impl fmt::Debug for RecvBodyMode {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::LengthDelimited(arg0) => {
                    f.debug_tuple("LengthDelimited").field(arg0).finish()
                }
                Self::Chunked => write!(f, "Chunked"),
                Self::CloseDelimited => write!(f, "CloseDelimited"),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_recv_no_headers() -> Result<()> {
        let mut buf = [0; 1024];
        let r: Response<RECV_RESPONSE> = Response::new_test();

        let a = r.try_read_response(b"HTTP/1.1 404\r\n\r\n", &mut buf)?;
        assert!(a.is_success());

        let status = a.status().unwrap();
        assert_eq!(status, &Status(HttpVersion::Http11, 404, ""));

        assert!(a.headers().unwrap().is_empty());
        Ok(())
    }
}
