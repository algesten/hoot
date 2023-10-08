use core::fmt::{self, Write};
use core::marker::PhantomData;
use core::mem;
use core::mem::align_of;
use core::mem::size_of;

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
    pub struct STATE_INIT;
    pub struct STATE_LINE;
    pub struct STATE_HEADERS;
    pub struct STATE_BODY;
    pub struct STATE_STATUS;
    pub struct STATE_HEADERS_RECV;
}

#[allow(non_camel_case_types)]
pub mod version {
    pub struct HTTP_10;
    pub struct HTTP_11;
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
    impl State for STATE_INIT {}
    impl State for STATE_LINE {}
    impl State for STATE_HEADERS {}
    impl State for STATE_BODY {}
    impl State for STATE_STATUS {}
    impl State for STATE_HEADERS_RECV {}

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
    pub fn new(buf: &'a mut [u8]) -> Call<'a, STATE_INIT, (), ()> {
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
    pub fn flush(self) -> (CallState<S, V, M>, &'a [u8]) {
        (self.state, self.out.flush())
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

impl<'a> Call<'a, STATE_INIT, (), ()> {
    pub fn http_10(self) -> Call<'a, STATE_LINE, HTTP_10, ()> {
        self.transition()
    }

    pub fn http_11(self) -> Call<'a, STATE_LINE, HTTP_11, ()> {
        self.transition()
    }
}

impl<'a> Call<'a, STATE_LINE, HTTP_10, ()> {
    pub fn get(mut self, path: &str) -> Result<Call<'a, STATE_HEADERS, HTTP_10, GET>> {
        write!(&mut self.out, "GET {} HTTP/1.0\r\n", path).or(OVERFLOW)?;
        Ok(self.transition())
    }

    pub fn head(mut self, path: &str) -> Result<Call<'a, STATE_HEADERS, HTTP_10, HEAD>> {
        write!(&mut self.out, "HEAD {} HTTP/1.0\r\n", path).or(OVERFLOW)?;
        Ok(self.transition())
    }

    pub fn post(mut self, path: &str) -> Result<Call<'a, STATE_HEADERS, HTTP_10, POST>> {
        write!(&mut self.out, "POST {} HTTP/1.0\r\n", path).or(OVERFLOW)?;
        Ok(self.transition())
    }
}

impl<'a, M: Method> Call<'a, STATE_HEADERS, HTTP_10, M> {
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

impl<'a, M: MethodWithBody> Call<'a, STATE_HEADERS, HTTP_10, M> {
    pub fn with_body(mut self, length: u64) -> Result<Call<'a, STATE_BODY, HTTP_10, M>> {
        write!(&mut self.out, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, STATE_STATUS, HTTP_10, M>> {
        write!(&mut self.out, "\r\n").or(OVERFLOW)?;
        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithoutBody> Call<'a, STATE_HEADERS, V, M> {
    pub fn finish(mut self) -> Result<Call<'a, STATE_STATUS, HTTP_10, M>> {
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

impl<'a, V: Version, M: Method> Call<'a, STATE_STATUS, V, M> {
    pub fn parse_status<'b>(
        self,
        buf: &'b [u8],
    ) -> Result<ParseResult<'a, STATE_STATUS, V, M, STATE_HEADERS_RECV, Status<'b>>> {
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
}

impl<'a> fmt::Write for Out<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write(s.as_bytes()).and(Ok(())).or(Err(fmt::Error))
    }
}
