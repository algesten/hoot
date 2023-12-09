use core::fmt::Write;

use crate::vars::private;
use crate::Result;
use crate::{Call, CallState, Output, OVERFLOW};

use crate::method::*;
use crate::state::*;
use crate::version::*;
use private::*;

impl<'a, S, V, M> Output<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    pub fn ready(self) -> CallState<S, V, M> {
        self.state
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.output
    }
}

impl<'a> Call<'a, INIT, (), ()> {
    pub fn http_10(self) -> Call<'a, SEND_LINE, HTTP_10, ()> {
        self.transition()
    }

    pub fn http_11(self) -> Call<'a, SEND_LINE, HTTP_11, ()> {
        self.transition()
    }
}

macro_rules! send_method {
    ($meth:ident, $meth_up:tt, $ver:ty) => {
        pub fn $meth(mut self, path: &str) -> Result<Call<'a, SEND_HEADERS, $ver, $meth_up>> {
            self.out
                .write_send_line(stringify!($meth_up), path, <$ver>::version_str())?;
            Ok(self.transition())
        }
    };
}

impl<'a> Call<'a, SEND_LINE, HTTP_10, ()> {
    send_method!(get, GET, HTTP_10);
    send_method!(head, HEAD, HTTP_10);
    send_method!(post, POST, HTTP_10);
}

impl<'a> Call<'a, SEND_LINE, HTTP_11, ()> {
    send_method!(get, GET, HTTP_11);
    send_method!(head, HEAD, HTTP_11);
    send_method!(post, POST, HTTP_11);
    send_method!(put, PUT, HTTP_11);
    send_method!(delete, DELETE, HTTP_11);
    // CONNECT
    send_method!(options, OPTIONS, HTTP_11);
    send_method!(trace, TRACE, HTTP_11);
}

impl<'a, M: Method, V: Version> Call<'a, SEND_HEADERS, V, M> {
    pub fn header(mut self, name: &str, value: &str) -> Result<Self> {
        let mut w = self.out.writer();
        write!(w, "{}: {}\r\n", name, value).or(OVERFLOW)?;
        drop(w);
        Ok(self)
    }

    pub fn header_bytes(mut self, name: &str, bytes: &[u8]) -> Result<Self> {
        let mut w = self.out.writer();
        write!(w, "{}: ", name).or(OVERFLOW)?;
        w.write_bytes(bytes)?;
        write!(w, "\r\n").or(OVERFLOW)?;
        drop(w);
        Ok(self)
    }
}

impl<'a, M: MethodWithBody> Call<'a, SEND_HEADERS, HTTP_10, M> {
    pub fn with_body(mut self, length: u64) -> Result<Call<'a, SEND_BODY, HTTP_10, M>> {
        let mut w = self.out.writer();
        write!(w, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        drop(w);
        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_10, M>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        drop(w);
        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithoutBody> Call<'a, SEND_HEADERS, V, M> {
    pub fn finish(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_10, M>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        drop(w);
        Ok(self.transition())
    }
}
