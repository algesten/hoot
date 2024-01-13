use core::fmt::Write;
use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;

use crate::error::OVERFLOW;
use crate::header::check_and_output_header;
use crate::out::{Out, Writer};
use crate::util::LengthChecker;
use crate::vars::body::*;
use crate::vars::method::*;
use crate::vars::private::*;
use crate::vars::state::*;
use crate::vars::version::*;
use crate::Method as M;
use crate::{CallState, HttpVersion};
use crate::{HootError, Result};

use super::Response;

pub struct Request<'a, S: State, V: Version, M: Method, B: BodyType> {
    typ: Typ<S, V, M, B>,
    state: CallState,
    out: Out<'a>,
}

/// Zero sized struct only to hold type state.
#[derive(Default)]
struct Typ<S: State, V: Version, M: Method, B: BodyType>(
    PhantomData<S>,
    PhantomData<V>,
    PhantomData<M>,
    PhantomData<B>,
);

pub struct ResumeToken<S: State, V: Version, M: Method, B: BodyType> {
    typ: Typ<S, V, M, B>,
    state: CallState,
}

pub struct Output<'a, S: State, V: Version, M: Method, B: BodyType> {
    token: ResumeToken<S, V, M, B>,
    output: &'a [u8],
}

impl<'a> Request<'a, (), (), (), ()> {
    pub fn new(buf: &'a mut [u8]) -> Request<'a, INIT, (), (), ()> {
        let typ: Typ<(), (), (), ()> = Typ::default();
        Request {
            typ,
            state: CallState::default(),
            out: Out::wrap(buf),
        }
        .transition()
    }
}

impl<'a, S: State, V: Version, M: Method, B: BodyType> Request<'a, S, V, M, B> {
    fn transition<S2: State, V2: Version, M2: Method, B2: BodyType>(
        self,
    ) -> Request<'a, S2, V2, M2, B2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }

    fn header_raw(mut self, name: &str, bytes: &[u8], trailer: bool) -> Result<Self> {
        // Attempt writing the header
        let w = self.out.writer();
        check_and_output_header(w, V::version(), name, bytes, trailer)?;
        Ok(self)
    }

    pub fn flush(self) -> Output<'a, S, V, M, B> {
        Output {
            token: ResumeToken {
                typ: self.typ,
                state: self.state,
            },
            output: self.out.into_inner(),
        }
    }

    pub fn resume(token: ResumeToken<S, V, M, B>, buf: &'a mut [u8]) -> Request<'a, S, V, M, B> {
        Request {
            typ: token.typ,
            state: token.state,
            out: Out::wrap(buf),
        }
    }
}

impl<'a, S: State, V: Version, M: Method, B: BodyType> Output<'a, S, V, M, B> {
    pub fn ready(self) -> ResumeToken<S, V, M, B> {
        self.token
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.output
    }
}

impl<'a, S: State, V: Version, M: Method, B: BodyType> Deref for Output<'a, S, V, M, B> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

impl<S: State, V: Version, M: Method, B: BodyType> ResumeToken<S, V, M, B> {
    pub(crate) fn into_state(self) -> CallState {
        self.state
    }
}

impl<'a> Request<'a, INIT, (), (), ()> {
    pub fn http_10(mut self) -> Request<'a, SEND_LINE, HTTP_10, (), ()> {
        self.state.version = Some(HttpVersion::Http10);
        self.transition()
    }

    pub fn http_11(mut self) -> Request<'a, SEND_LINE, HTTP_11, (), ()> {
        self.state.version = Some(HttpVersion::Http11);
        self.transition()
    }
}

macro_rules! write_line_10 {
    ($meth:ident, $meth_up:tt) => {
        pub fn $meth(
            mut self,
            path: &str,
        ) -> Result<Request<'a, SEND_HEADERS, HTTP_10, $meth_up, ()>> {
            write_line_10(self.out.writer(), stringify!($meth_up), path)?;
            self.state.method = Some(M::$meth_up);
            Ok(self.transition())
        }
    };
}

fn write_line_10(mut w: Writer<'_, '_>, method: &str, path: &str) -> Result<()> {
    write!(w, "{} {} HTTP/1.0\r\n", method, path).or(OVERFLOW)?;
    w.commit();
    Ok(())
}

macro_rules! write_line_11 {
    ($meth:ident, $meth_up:tt) => {
        pub fn $meth(
            mut self,
            host: &str,
            path: &str,
        ) -> Result<Request<'a, SEND_HEADERS, HTTP_11, $meth_up, ()>> {
            write_line_11(self.out.writer(), stringify!($meth_up), host, path)?;
            self.state.method = Some(M::$meth_up);
            Ok(self.transition())
        }
    };
}

fn write_line_11(mut w: Writer<'_, '_>, method: &str, host: &str, path: &str) -> Result<()> {
    write!(w, "{} {} HTTP/1.1\r\nHost: {}\r\n", method, path, host).or(OVERFLOW)?;
    w.commit();
    Ok(())
}

impl<'a> Request<'a, SEND_LINE, HTTP_10, (), ()> {
    write_line_10!(get, GET);
    write_line_10!(head, HEAD);
    write_line_10!(post, POST);
}

impl<'a> Request<'a, SEND_LINE, HTTP_11, (), ()> {
    write_line_11!(get, GET);
    write_line_11!(head, HEAD);
    write_line_11!(post, POST);
    write_line_11!(put, PUT);
    write_line_11!(delete, DELETE);
    write_line_11!(connect, CONNECT);
    write_line_11!(options, OPTIONS);
    write_line_11!(trace, TRACE);
    write_line_11!(patch, PATCH);
}

impl<'a, M: Method, V: Version> Request<'a, SEND_HEADERS, V, M, ()> {
    pub fn header(self, name: &str, value: &str) -> Result<Self> {
        self.header_raw(name, value.as_bytes(), false)
    }

    pub fn header_bytes(self, name: &str, bytes: &[u8]) -> Result<Self> {
        self.header_raw(name, bytes, false)
    }
}

impl<'a, M: MethodWithRequestBody> Request<'a, SEND_HEADERS, HTTP_10, M, ()> {
    pub fn with_body(
        mut self,
        length: u64,
    ) -> Result<Request<'a, SEND_BODY, HTTP_10, M, BODY_LENGTH>> {
        let mut w = self.out.writer();
        write!(w, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        w.commit();

        self.state.send_checker = Some(LengthChecker::new(length));

        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Request<'a, RECV_RESPONSE, HTTP_11, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, M: MethodWithRequestBody> Request<'a, SEND_HEADERS, HTTP_11, M, ()> {
    pub fn with_body(
        mut self,
        length: u64,
    ) -> Result<Request<'a, SEND_BODY, HTTP_11, M, BODY_LENGTH>> {
        let mut w = self.out.writer();
        write!(w, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        w.commit();

        self.state.send_checker = Some(LengthChecker::new(length));

        Ok(self.transition())
    }

    pub fn with_chunked(mut self) -> Result<Request<'a, SEND_BODY, HTTP_11, M, BODY_CHUNKED>> {
        let mut w = self.out.writer();
        write!(w, "Transfer-Encoding: chunked\r\n\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Request<'a, RECV_RESPONSE, HTTP_11, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithoutRequestBody> Request<'a, SEND_HEADERS, V, M, ()> {
    // TODO: Can we find a trait bound that allows us to call this without_body()?
    pub fn send(mut self) -> Result<Request<'a, ENDED, (), (), ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithRequestBody> Request<'a, SEND_BODY, V, M, BODY_LENGTH> {
    #[inline(always)]
    fn checker(&mut self) -> &mut LengthChecker {
        self.state
            .send_checker
            .as_mut()
            // If we don't have the checker when in type state SEND_BODY, we got a bug.
            .expect("SendByteCheck when SEND_BODY")
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        // This returns Err if we try to write more bytes than content-length.
        self.checker()
            .append(bytes.len(), HootError::SentMoreThanContentLength)?;

        let mut w = self.out.writer();
        w.write_bytes(bytes)?;
        w.commit();

        Ok(())
    }

    pub fn finish(mut self) -> Result<Request<'a, ENDED, (), (), ()>> {
        // This returns Err if we have written less than content-length.
        self.checker()
            .assert_expected(HootError::SentLessThanContentLength)?;

        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithRequestBody> Request<'a, SEND_BODY, V, M, BODY_CHUNKED> {
    pub fn write_chunk(mut self, bytes: &[u8]) -> Result<Self> {
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

    pub fn with_trailer(mut self) -> Result<Request<'a, SEND_TRAILER, V, M, BODY_CHUNKED>> {
        let mut w = self.out.writer();
        write!(w, "0\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }

    pub fn finish(mut self) -> Result<Request<'a, ENDED, (), (), ()>> {
        let mut w = self.out.writer();
        write!(w, "0\r\n\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

// TODO: ensure trailers are declared in a `Trailer: xxx` header.
impl<'a, V: Version, M: MethodWithRequestBody> Request<'a, SEND_TRAILER, V, M, BODY_CHUNKED> {
    pub fn trailer(self, name: &str, value: &str) -> Result<Self> {
        self.header_raw(name, value.as_bytes(), true)
    }

    pub fn trailer_bytes(self, name: &str, bytes: &[u8]) -> Result<Self> {
        self.header_raw(name, bytes, true)
    }

    pub fn finish(mut self) -> Result<Request<'a, ENDED, (), (), ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a> Output<'a, ENDED, (), (), ()> {
    pub fn into_response(self) -> Response<RECV_RESPONSE> {
        self.token.into_response()
    }
}

impl ResumeToken<ENDED, (), (), ()> {
    pub fn into_response(self) -> Response<RECV_RESPONSE> {
        Response::resume(self)
    }
}

#[cfg(feature = "std")]
mod std_impls {
    use super::*;
    use std::fmt;

    impl<'a, S: State, V: Version, M: Method, B: BodyType> fmt::Debug for Request<'a, S, V, M, B> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Request").finish()
        }
    }

    impl fmt::Debug for CallState {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("CallState")
                .field("method", &self.method)
                .field("send_checker", &self.send_checker)
                .field("recv_body_mode", &self.recv_body_mode)
                .field("recv_checker", &self.recv_checker)
                .field("did_read_to_end", &self.did_read_to_end)
                .finish()
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod test {
    use super::*;
    use crate::HootError;

    #[test]
    pub fn test_illegal_header_name() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Request::new(&mut buf)
            .http_11()
            .get("myhost.test:8080", "/path")?
            .header(":bad:", "fine value");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::HeaderName);

        Ok(())
    }

    #[test]
    pub fn test_illegal_header_value() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Request::new(&mut buf)
            .http_11()
            .get("myhost.test:8080", "/path")?
            .header_bytes("x-broken", b"value\0xx");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::HeaderValue);

        Ok(())
    }
    #[test]
    pub fn test_illegal_body_header() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Request::new(&mut buf)
            .http_10()
            .get("/path")?
            .header("transfer-encoding", "chunked");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::ForbiddenBodyHeader);

        Ok(())
    }

    #[test]
    pub fn test_illegal_http11_header() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Request::new(&mut buf)
            .http_11()
            .get("myhost.test:8080", "/path")?
            .header("Host", "another.test:4489");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::ForbiddenHttp11Header);

        Ok(())
    }
}
