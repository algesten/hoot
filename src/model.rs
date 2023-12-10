use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;
use core::str;

use crate::out::Out;
use crate::util::compare_lowercase_ascii;
use crate::vars::private;

use crate::{state::*, HootError, Result};
use httparse::Header;
use private::*;

pub struct CallState<S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    _state: PhantomData<S>,
    _version: PhantomData<V>,
    _method: PhantomData<M>,
    _btype: PhantomData<B>,
    pub(crate) send_byte_checker: Option<SendByteChecker>,
    pub(crate) status_code: Option<u16>,
    pub(crate) recv_body_mode: Option<RecvBodyMode>,
}

pub(crate) struct SendByteChecker {
    sent: u64,
    expected: u64,
}

impl SendByteChecker {
    pub(crate) fn new(expected: u64) -> Self {
        SendByteChecker { sent: 0, expected }
    }

    pub(crate) fn append(&mut self, sent: usize) -> Result<()> {
        let new_total = self.sent + sent as u64;
        if new_total > self.expected {
            return Err(HootError::SentMoreThanContentLength);
        }
        self.sent = new_total;
        Ok(())
    }

    pub(crate) fn assert_expected(&self) -> Result<()> {
        if self.sent != self.expected {
            return Err(HootError::SentLessThanContentLength);
        }

        Ok(())
    }
}

// #[derive(Copy, Clone, Debug, PartialEq, Eq)]
// pub(crate) enum BodyTypeRecv {
//     NoBody,
//     LengthDelimited(u64),
//     Chunked,
//     CloseDelimited,
// }

impl CallState<(), (), (), ()> {
    fn new<S: State, V: Version, M: Method, B: BodyType>() -> CallState<S, V, M, B> {
        CallState {
            _state: PhantomData,
            _version: PhantomData,
            _method: PhantomData,
            _btype: PhantomData,
            send_byte_checker: None,
            status_code: None,
            recv_body_mode: None,
        }
    }
}

pub struct Call<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub(crate) state: CallState<S, V, M, B>,
    pub(crate) out: Out<'a>,
}

impl<'a> Call<'a, (), (), (), ()> {
    pub fn new(buf: &'a mut [u8]) -> Call<'a, INIT, (), (), ()> {
        Call {
            state: CallState::new(),
            out: Out::wrap(buf),
        }
    }
}

impl<'a, S, V, M, B> Call<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub fn flush(self) -> Output<'a, S, V, M, B> {
        Output {
            state: self.state,
            output: self.out.flush(),
        }
    }

    pub fn resume(state: CallState<S, V, M, B>, buf: &'a mut [u8]) -> Call<'a, S, V, M, B> {
        Call {
            state,
            out: Out::wrap(buf),
        }
    }

    pub(crate) fn transition<S2: State, V2: Version, M2: Method, B2: BodyType>(
        self,
    ) -> Call<'a, S2, V2, M2, B2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }
}

pub struct Output<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub(crate) state: CallState<S, V, M, B>,
    pub(crate) output: &'a [u8],
}

impl<'a, S, V, M, B> Output<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub fn ready(self) -> CallState<S, V, M, B> {
        self.state
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.output
    }
}

impl<'a, S, V, M, B> Deref for Output<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Status<'a>(pub HttpVersion, pub u16, pub &'a str);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
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

    impl fmt::Debug for HttpVersion {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Http10 => write!(f, "Http10"),
                Self::Http11 => write!(f, "Http11"),
            }
        }
    }

    impl<'a, S, V, M, B> fmt::Debug for Call<'a, S, V, M, B>
    where
        S: State,
        V: Version,
        M: Method,
        B: BodyType,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Call").finish()
        }
    }

    impl fmt::Debug for Status<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("Status")
                .field(&self.0)
                .field(&self.1)
                .field(&self.2)
                .finish()
        }
    }
}
