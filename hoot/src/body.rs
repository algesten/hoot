use core::fmt;
use core::ops::Deref;
use core::str;

use crate::chunk::Dechunker;
use crate::error::Result;
use crate::util::compare_lowercase_ascii;
use crate::{CallState, HootError, Method};

pub(crate) fn do_read_body<'a, 'b>(
    state: &mut CallState,
    src: &'a [u8],
    dst: &'b mut [u8],
) -> Result<BodyPart<'b>> {
    trace!("Read body");

    // If we already read to completion, do not use any more input.
    if state.did_read_to_end {
        return Ok(BodyPart::empty());
    }

    // unwrap is ok because we can't be in state RECV_BODY without setting it.
    let part = match state.recv_body_mode.unwrap() {
        RecvBodyMode::LengthDelimited(_) => read_limit(state, src, dst, true),
        RecvBodyMode::Chunked => read_chunked(state, src, dst),
        RecvBodyMode::CloseDelimited => read_limit(state, src, dst, false),
    }?;

    if part.finished {
        state.did_read_to_end = true;
    }

    Ok(part)
}

fn read_limit<'a, 'b>(
    state: &mut CallState,
    src: &'a [u8],
    dst: &'b mut [u8],
    use_checker: bool,
) -> Result<BodyPart<'b>> {
    let mut input_used = src.len().min(dst.len());

    let mut finished = false;
    if use_checker {
        // unwrap is ok, because use_checker can't be true if we haven't got
        // a length checker set for the response.
        let checker = state.recv_checker.as_mut().unwrap();

        // the input we need to read to fulfil the length checker might be
        // smaller than the input buffers
        input_used = checker.left_to_read().min(input_used);

        checker.append(input_used, HootError::RecvMoreThanContentLength)?;
        finished = checker.complete();

        trace!("Read body limited: {}", input_used);
    } else {
        trace!("Read body closed: {}", input_used);
    }

    let data = &mut dst[..input_used];
    data.copy_from_slice(&src[..input_used]);

    Ok(BodyPart {
        input_used,
        data,
        finished,
    })
}

fn read_chunked<'a>(state: &mut CallState, src: &[u8], dst: &'a mut [u8]) -> Result<BodyPart<'a>> {
    if state.dechunker.is_none() {
        state.dechunker = Some(Dechunker::new());
    }
    let dechunker = state.dechunker.as_mut().unwrap();
    let (input_used, produced_output) = dechunker.parse_input(src, dst)?;

    let data = &mut dst[..produced_output];
    let finished = dechunker.is_ended();

    trace!("Read chunked: {}", input_used);

    Ok(BodyPart {
        input_used,
        data,
        finished,
    })
}

pub struct BodyPart<'b> {
    pub(crate) input_used: usize,
    pub(crate) data: &'b [u8],
    pub(crate) finished: bool,
}

impl BodyPart<'_> {
    pub fn input_used(&self) -> usize {
        self.input_used
    }

    pub fn data(&self) -> &[u8] {
        self.data
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

impl BodyPart<'_> {
    pub(crate) fn empty() -> Self {
        BodyPart {
            input_used: 0,
            data: &[],
            finished: false,
        }
    }
}

impl Deref for BodyPart<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
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
    pub fn for_request<'a>(
        http10: bool,
        method: Method,
        header_lookup: &'a dyn Fn(&str) -> Option<&'a str>,
    ) -> Result<Self> {
        let has_no_body = !method.has_request_body();

        if has_no_body {
            return Ok(Self::LengthDelimited(0));
        }

        let ret = match Self::header_defined(http10, header_lookup)? {
            // Request bodies cannot be close delimited (even under http10).
            Self::CloseDelimited => Self::LengthDelimited(0),
            r @ _ => r,
        };

        Ok(ret)
    }

    pub fn for_response<'a>(
        http10: bool,
        method: Method,
        status_code: u16,
        header_lookup: &'a dyn Fn(&str) -> Option<&'a str>,
    ) -> Result<Self> {
        let is_success = status_code >= 200 && status_code <= 299;
        let is_informational = status_code >= 100 && status_code <= 199;

        let has_no_body =
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
            // All responses to the HEAD request method
            // MUST NOT include a message-body, even though the presence of entity-
            // header fields might lead one to believe they do.
            method == Method::HEAD ||
            // A client MUST ignore any Content-Length or Transfer-Encoding
            // header fields received in a successful response to CONNECT.
            is_success && method == Method::CONNECT ||
            // All 1xx (informational), 204 (no content), and 304 (not modified) responses
            // MUST NOT include a message-body.
            is_informational ||
            matches!(status_code, 204 | 304);

        if has_no_body {
            if http10 {
                return Ok(Self::CloseDelimited);
            } else {
                return Ok(Self::LengthDelimited(0));
            }
        }

        // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
        // All other responses do include a message-body, although it MAY be of zero length.
        Self::header_defined(http10, header_lookup)
    }

    fn header_defined<'a>(
        http10: bool,
        header_lookup: &'a dyn Fn(&str) -> Option<&'a str>,
    ) -> Result<Self> {
        let mut content_length: Option<u64> = None;
        let mut chunked = false;

        // for head in headers {
        if let Some(value) = header_lookup("content-length") {
            let v = value.parse::<u64>()?;
            if content_length.is_some() {
                return Err(HootError::DuplicateContentLength);
            }
            content_length = Some(v);
        }

        if let Some(value) = header_lookup("transfer-encoding") {
            // Header can repeat, stop looking if we found "chunked"
            chunked = value
                .split(",")
                .map(|v| v.trim())
                .any(|v| compare_lowercase_ascii(v, "chunked"));
        }

        if chunked && !http10 {
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

impl fmt::Debug for RecvBodyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthDelimited(arg0) => f.debug_tuple("LengthDelimited").field(arg0).finish(),
            Self::Chunked => write!(f, "Chunked"),
            Self::CloseDelimited => write!(f, "CloseDelimited"),
        }
    }
}
