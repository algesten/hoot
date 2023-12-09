use core::fmt::{self, Write};
use core::marker::PhantomData;
use core::mem;
use core::mem::align_of;
use core::mem::size_of;
use core::ops::Deref;

use crate::H1Error;
use crate::Result;

use httparse::Header;
use private::{Method, State, Version};

use self::method::*;
use self::private::*;
use self::state::*;
use self::version::*;

pub struct CallState<S, V, M>(PhantomData<S>, PhantomData<V>, PhantomData<M>)
where
    S: State,
    V: Version,
    M: Method;

impl CallState<(), (), ()> {
    fn new<S: State, V: Version, M: Method>() -> CallState<S, V, M> {
        CallState(PhantomData, PhantomData, PhantomData)
    }
}

#[allow(non_camel_case_types)]
pub mod state {
    pub struct INIT;
    pub struct SEND_LINE;
    pub struct SEND_HEADERS;
    pub struct SEND_BODY;
    pub struct RECV_STATUS;
    pub struct RECV_HEADERS;
}

#[allow(non_camel_case_types)]
pub mod version {
    pub struct HTTP_10;
    pub struct HTTP_11;
}

impl HTTP_10 {
    const fn version_str() -> &'static str {
        "1.0"
    }
}

impl HTTP_11 {
    const fn version_str() -> &'static str {
        "1.1"
    }
}

#[allow(non_camel_case_types)]
pub mod method {
    pub struct OPTIONS;
    pub struct GET;
    pub struct POST;
    pub struct PUT;
    pub struct DELETE;
    pub struct HEAD;
    pub struct TRACE;
    pub struct CONNECT;
    pub struct PATCH;
}

mod private {
    use super::method::*;
    use super::state::*;
    use super::version::*;

    pub trait State {}
    pub trait Version {
        fn httparse_version() -> u8;
    }
    pub trait Method {}

    impl State for () {}
    impl State for INIT {}
    impl State for SEND_LINE {}
    impl State for SEND_HEADERS {}
    impl State for SEND_BODY {}
    impl State for RECV_STATUS {}
    impl State for RECV_HEADERS {}

    impl Version for () {
        fn httparse_version() -> u8 {
            unreachable!()
        }
    }
    impl Version for HTTP_10 {
        fn httparse_version() -> u8 {
            0
        }
    }
    impl Version for HTTP_11 {
        fn httparse_version() -> u8 {
            1
        }
    }

    impl Method for () {}
    impl Method for OPTIONS {}
    impl Method for GET {}
    impl Method for POST {}
    impl Method for PUT {}
    impl Method for DELETE {}
    impl Method for HEAD {}
    impl Method for TRACE {}
    impl Method for CONNECT {}
    impl Method for PATCH {}

    pub trait MethodWithBody: Method {}

    impl MethodWithBody for POST {}
    impl MethodWithBody for PUT {}
    impl MethodWithBody for PATCH {}

    pub trait MethodWithoutBody: Method {}
    impl MethodWithoutBody for OPTIONS {}
    impl MethodWithoutBody for GET {}
    impl MethodWithoutBody for DELETE {}
    impl MethodWithoutBody for HEAD {}
    impl MethodWithoutBody for TRACE {}
    impl MethodWithoutBody for CONNECT {}
}

static OVERFLOW: Result<()> = Err(H1Error::OutputOverflow);

pub struct Call<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    state: CallState<S, V, M>,
    out: Out<'a>,
}

impl<'a> Call<'a, (), (), ()> {
    pub fn new(buf: &'a mut [u8]) -> Call<'a, INIT, (), ()> {
        Call {
            state: CallState::new(),
            out: Out::wrap(buf),
        }
    }
}

impl<'a, S, V, M> Call<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    pub fn flush(self) -> Output<'a, S, V, M> {
        Output {
            state: self.state,
            output: self.out.flush(),
        }
    }

    pub fn resume(state: CallState<S, V, M>, buf: &'a mut [u8]) -> Call<'a, S, V, M> {
        Call {
            state,
            out: Out::wrap(buf),
        }
    }

    fn transition<S2: State, V2: Version, M2: Method>(self) -> Call<'a, S2, V2, M2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }
}

pub struct Output<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    state: CallState<S, V, M>,
    output: &'a [u8],
}

impl<'a, S, V, M> Deref for Output<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

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
        write!(&mut self.out, "{}: {}\r\n", name, value).or(OVERFLOW)?;
        Ok(self)
    }

    pub fn header_bytes(mut self, name: &str, bytes: &[u8]) -> Result<Self> {
        write!(&mut self.out, "{}: ", name).or(OVERFLOW)?;
        self.out.write(bytes)?;
        write!(&mut self.out, "\r\n").or(OVERFLOW)?;
        Ok(self)
    }
}

impl<'a, M: MethodWithBody> Call<'a, SEND_HEADERS, HTTP_10, M> {
    pub fn with_body(mut self, length: u64) -> Result<Call<'a, SEND_BODY, HTTP_10, M>> {
        write!(&mut self.out, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_10, M>> {
        write!(&mut self.out, "\r\n").or(OVERFLOW)?;
        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithoutBody> Call<'a, SEND_HEADERS, V, M> {
    pub fn finish(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_10, M>> {
        write!(&mut self.out, "\r\n").or(OVERFLOW)?;
        Ok(self.transition())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status<'a>(pub u16, pub Option<&'a str>);

pub enum ParseResult<'a, S1: State, V: Version, M: Method, S2: State, T> {
    Incomplete(Call<'a, S1, V, M>),
    Complete(Call<'a, S2, V, M>, usize, T),
}

impl<'a, V: Version, M: Method> Call<'a, RECV_STATUS, V, M> {
    pub fn parse_status<'b>(
        self,
        buf: &'b [u8],
    ) -> Result<ParseResult<'a, RECV_STATUS, V, M, RECV_HEADERS, Status<'b>>> {
        let mut response = {
            // Borrow the byte buffer temporarily for header parsing.
            let tmp = &mut self.out.buf[self.out.pos..];
            let headers = buf_for_headers(tmp);
            httparse::Response::new(headers)
        };

        response.parse(buf)?;

        // HTTP/1.0 200 OK
        let (Some(version), Some(code)) = (response.version, response.code) else {
            return Ok(ParseResult::Incomplete(self));
        };

        if version != V::httparse_version() {
            return Err(H1Error::InvalidHttpVersion);
        }

        let status = Status(code, response.reason);

        Ok(ParseResult::Complete(self.transition(), 0, status))
    }
}

fn buf_for_headers<'h, 'b>(buf: &'h mut [u8]) -> &'h mut [Header<'b>] {
    let byte_len = buf.len();

    // The alignment of Header
    let align = align_of::<httparse::Header>();

    // Treat buffer as a pointer to Header
    let ptr = buf.as_mut_ptr() as *mut Header;

    // The amount of offset needed to be aligned.
    let offset = ptr.align_offset(align);

    if offset >= byte_len {
        panic!("Not possible to align header buffer");
    }

    // The number of Header elements we can fit in the buffer.
    let len = (byte_len - offset) / size_of::<httparse::Header>();

    // Move pointer to alignment
    // SAFETY: We checked above that this is within bounds.
    let ptr = unsafe { ptr.add(offset) };

    // SAFETY: Yolo
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}

struct Out<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Out<'a> {
    fn wrap(buf: &'a mut [u8]) -> Self {
        Out { buf, pos: 0 }
    }

    fn write<'b>(&mut self, bytes: &'b [u8]) -> Result<usize> {
        if bytes.len() >= self.buf.len() {
            return Err(H1Error::OutputOverflow);
        }

        self.buf[self.pos..(self.pos + bytes.len())].copy_from_slice(bytes);
        self.pos += bytes.len();

        Ok(bytes.len())
    }

    fn flush(self) -> &'a [u8] {
        &self.buf[..self.pos]
    }

    fn write_send_line(&mut self, method: &str, path: &str, version: &str) -> Result<()> {
        write!(self, "{} {} HTTP/{}\r\n", method, path, version).or(OVERFLOW)
    }
}

impl<'a> fmt::Write for Out<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write(s.as_bytes()).and(Ok(())).or(Err(fmt::Error))
    }
}
