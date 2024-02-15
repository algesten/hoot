use core::fmt::Write;
use core::marker::PhantomData;
use core::ops::Deref;

use crate::error::{Result, OVERFLOW};
use crate::header::check_and_output_header;
use crate::out::Out;
use crate::types::body::*;
use crate::types::method::*;
use crate::types::state::*;
use crate::types::*;
use crate::util::LengthChecker;
use crate::{CallState, HootError, HttpVersion};

pub enum ResponseVariant {
    Get(ResumeToken<SEND_STATUS, GET, ()>),
    Head(ResumeToken<SEND_STATUS, HEAD, ()>),
    Post(ResumeToken<SEND_STATUS, POST, ()>),
    Put(ResumeToken<SEND_STATUS, PUT, ()>),
    Delete(ResumeToken<SEND_STATUS, DELETE, ()>),
    Connect(ResumeToken<SEND_STATUS, CONNECT, ()>),
    Options(ResumeToken<SEND_STATUS, OPTIONS, ()>),
    Trace(ResumeToken<SEND_STATUS, TRACE, ()>),
    Patch(ResumeToken<SEND_STATUS, PATCH, ()>),
}

pub struct Response<'a, S: State, M: Method, B: BodyType> {
    typ: Typ<S, M, B>,
    state: CallState,
    out: Out<'a>,
}

/// Zero sized struct only to hold type state.
#[derive(Default)]
struct Typ<S: State, M: Method, B: BodyType>(
    //
    PhantomData<S>,
    PhantomData<M>,
    PhantomData<B>,
);

pub struct ResumeToken<S: State, M: Method, B: BodyType> {
    typ: Typ<S, M, B>,
    state: CallState,
}

impl ResumeToken<(), (), ()> {
    pub(crate) fn new<M: Method>(state: CallState) -> ResumeToken<SEND_STATUS, M, ()> {
        ResumeToken {
            typ: Typ(PhantomData, PhantomData, PhantomData),
            state,
        }
    }
}

pub struct Output<'a, S: State, M: Method, B: BodyType> {
    token: ResumeToken<S, M, B>,
    output: &'a [u8],
}

impl<'a, S: State, M: Method, B: BodyType> Response<'a, S, M, B> {
    fn transition<S2: State, M2: Method, B2: BodyType>(self) -> Response<'a, S2, M2, B2> {
        trace!(
            "Transition: {}/{}/{} -> {}/{}/{}",
            S::state_name(),
            M::state_name(),
            B::state_name(),
            S2::state_name(),
            M2::state_name(),
            B2::state_name(),
        );

        Response {
            typ: Typ(PhantomData, PhantomData, PhantomData),
            state: self.state,
            out: self.out,
        }
    }

    fn header_raw(mut self, name: &str, bytes: &[u8], trailer: bool) -> Result<Self> {
        let ver = self.state.version.unwrap();
        // Attempt writing the header
        let w = self.out.writer();
        check_and_output_header(w, ver, name, bytes, trailer)?;
        Ok(self)
    }

    pub fn flush(self) -> Output<'a, S, M, B> {
        trace!("Flush");
        Output {
            token: ResumeToken {
                typ: self.typ,
                state: self.state,
            },
            output: self.out.into_inner(),
        }
    }

    pub fn resume(token: ResumeToken<S, M, B>, buf: &'a mut [u8]) -> Response<'a, S, M, B> {
        trace!(
            "Resume in state {}/{}/{}",
            S::state_name(),
            M::state_name(),
            B::state_name()
        );
        Response {
            typ: token.typ,
            state: token.state,
            out: Out::wrap(buf),
        }
    }
}

impl<'a, M: Method> Response<'a, SEND_STATUS, M, ()> {
    pub fn send_status(
        mut self,
        code: u16,
        text: &str,
    ) -> Result<Response<'a, SEND_HEADERS, M, ()>> {
        // Unwrap is OK, because the request must have set the version.
        let ver = match self.state.version.unwrap() {
            HttpVersion::Http10 => "1.0",
            HttpVersion::Http11 => "1.1",
        };

        trace!("Send status: {} {} HTTP/{}", code, text, ver);

        let mut w = self.out.writer();
        write!(w, "HTTP/{} {} {}\r\n", ver, code, text).or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, M: Method> Response<'a, SEND_HEADERS, M, ()> {
    pub fn header(self, name: &str, value: &str) -> Result<Self> {
        trace!("Set header {}: {}", name, value);
        self.header_raw(name, value.as_bytes(), false)
    }

    pub fn header_bytes(self, name: &str, bytes: &[u8]) -> Result<Self> {
        trace!("Set header bytes {}: {:?}", name, bytes);
        self.header_raw(name, bytes, false)
    }
}

impl<'a, M: MethodWithResponseBody> Response<'a, SEND_HEADERS, M, ()> {
    pub fn with_body(
        mut self,
        length: impl TryInto<u64>,
    ) -> Result<Response<'a, SEND_BODY, M, BODY_LENGTH>> {
        let length: u64 = length.try_into().map_err(|_| HootError::NotU64)?;

        trace!("Length delimited body: {}", length);

        let mut w = self.out.writer();
        write!(w, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        w.commit();

        self.state.send_checker = Some(LengthChecker::new(length));

        Ok(self.transition())
    }

    pub fn with_chunked(mut self) -> Result<Response<'a, SEND_BODY, M, BODY_CHUNKED>> {
        trace!("Chunked body");

        let mut w = self.out.writer();
        write!(w, "Transfer-Encoding: chunked\r\n\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Response<'a, RECV_RESPONSE, M, ()>> {
        trace!("Without body");

        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, M: MethodWithoutResponseBody> Response<'a, SEND_HEADERS, M, ()> {
    // TODO: Can we find a trait bound that allows us to call this without_body()?
    pub fn send(mut self) -> Result<Response<'a, ENDED, (), ()>> {
        trace!("Without body");

        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, M: MethodWithResponseBody> Response<'a, SEND_BODY, M, BODY_LENGTH> {
    #[inline(always)]
    fn checker(&mut self) -> &mut LengthChecker {
        self.state
            .send_checker
            .as_mut()
            // If we don't have the checker when in type state SEND_BODY, we got a bug.
            .expect("SendByteCheck when SEND_BODY")
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        trace!("Write bytes len: {}", bytes.len());

        // This returns Err if we try to write more bytes than content-length.
        self.checker()
            .append(bytes.len(), HootError::SentMoreThanContentLength)?;

        let mut w = self.out.writer();
        w.write_bytes(bytes)?;
        w.commit();

        Ok(())
    }

    pub fn finish(mut self) -> Result<Response<'a, ENDED, (), ()>> {
        trace!("Body finished");

        // This returns Err if we have written less than content-length.
        self.checker()
            .assert_expected(HootError::SentLessThanContentLength)?;

        Ok(self.transition())
    }
}

impl<'a, M: MethodWithResponseBody> Response<'a, SEND_BODY, M, BODY_CHUNKED> {
    pub fn write_chunk(mut self, bytes: &[u8]) -> Result<Self> {
        trace!("Write chunk len: {}", bytes.len());

        // Writing no bytes is ok. Ending the chunk writing is by doing the finish() call.
        if bytes.is_empty() {
            return Ok(self);
        }

        let mut w = self.out.writer();

        // chunk length
        write!(w, "{:0x?}\r\n", bytes.len()).or(OVERFLOW)?;

        // chunk
        w.write_bytes(bytes)?;

        // chunk end
        write!(w, "\r\n").or(OVERFLOW)?;

        w.commit();

        Ok(self)
    }

    pub fn with_trailer(mut self) -> Result<Response<'a, SEND_TRAILER, M, BODY_CHUNKED>> {
        trace!("With trailer");

        let mut w = self.out.writer();
        write!(w, "0\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }

    pub fn finish(mut self) -> Result<Response<'a, ENDED, (), ()>> {
        trace!("Body chunks finished");

        let mut w = self.out.writer();
        write!(w, "0\r\n\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

// TODO: ensure trailers are declared in a `Trailer: xxx` header.
impl<'a, M: MethodWithResponseBody> Response<'a, SEND_TRAILER, M, BODY_CHUNKED> {
    pub fn trailer(self, name: &str, value: &str) -> Result<Self> {
        trace!("Set trailer {}: {}", name, value);

        self.header_raw(name, value.as_bytes(), true)
    }

    pub fn trailer_bytes(self, name: &str, bytes: &[u8]) -> Result<Self> {
        trace!("Set trailer bytes {}: {:?}", name, bytes);

        self.header_raw(name, bytes, true)
    }

    pub fn finish(mut self) -> Result<Response<'a, ENDED, (), ()>> {
        trace!("Trailer finish");

        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, S: State, M: Method, B: BodyType> Output<'a, S, M, B> {
    pub fn ready(self) -> ResumeToken<S, M, B> {
        self.token
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.output
    }
}

impl<'a, S: State, M: Method, B: BodyType> Deref for Output<'a, S, M, B> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

impl From<CallState> for ResponseVariant {
    fn from(value: CallState) -> Self {
        // If we get an error on this unwrap, we have a bug.
        let method = value.method.unwrap();
        match method {
            crate::Method::OPTIONS => ResponseVariant::Options(ResumeToken::new(value)),
            crate::Method::GET => ResponseVariant::Get(ResumeToken::new(value)),
            crate::Method::POST => ResponseVariant::Post(ResumeToken::new(value)),
            crate::Method::PUT => ResponseVariant::Put(ResumeToken::new(value)),
            crate::Method::DELETE => ResponseVariant::Delete(ResumeToken::new(value)),
            crate::Method::HEAD => ResponseVariant::Head(ResumeToken::new(value)),
            crate::Method::TRACE => ResponseVariant::Trace(ResumeToken::new(value)),
            crate::Method::CONNECT => ResponseVariant::Connect(ResumeToken::new(value)),
            crate::Method::PATCH => ResponseVariant::Patch(ResumeToken::new(value)),
        }
    }
}
