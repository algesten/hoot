use std::fmt;
use std::io::Write;
use std::marker::PhantomData;

use http::{HeaderName, HeaderValue, Method, Request, Response, StatusCode, Version};
use httparse::Status;

use crate::analyze::RequestExt;
use crate::body::{BodyMode, BodyReader};
use crate::util::Writer;
use crate::Error;

use super::{RecvBody, RecvResponse, WithBody, WithoutBody};

/// An HTTP/1.1 call
#[repr(C)]
pub struct Call<'a, B> {
    holder: Holder<'a>,
    call_state: CallState,
    _ph: PhantomData<B>,
}

impl<'a> Call<'a, ()> {
    /// Creates a call for a [`Method`] that do not have a request body
    ///
    /// Methods like `HEAD` and `GET` do not use a request body. This creates
    /// a [`Call`] instance that does not expect a user provided body.
    ///
    /// ```
    /// # use hoot::client::Call;
    /// # use hoot::http::Request;
    /// let req = Request::head("http://foo.test/page").body(()).unwrap();
    /// Call::without_body(&req).unwrap();
    /// ```
    pub fn without_body(request: &'a Request<()>) -> Result<Call<'a, WithoutBody>, Error> {
        Call::new(request, BodyMode::None)
    }

    /// Creates a call for a [`Method`] that requires a request body
    ///
    /// Methods like `POST` and `PUT` expects a request body. This must be
    /// used even if the body is zero-sized (`content-length: 0`).
    ///
    /// ```
    /// # use hoot::client::Call;
    /// # use hoot::http::Request;
    /// let req = Request::put("http://foo.test/path").body(()).unwrap();
    /// Call::with_body(&req).unwrap();
    /// ```
    pub fn with_body(request: &'a Request<()>) -> Result<Call<'a, WithBody>, Error> {
        Call::new(request, BodyMode::Chunked)
    }
}

#[repr(C)]
struct Holder<'a> {
    preheaders: [Option<(HeaderName, HeaderValue)>; 2],
    request: Option<&'a Request<()>>,
    method: Method,
}

impl<'a> Holder<'a> {
    fn expect_request(&self) -> &Request<()> {
        // If this happens, we ar calling expect_request() in a state where it isn't expected.
        self.request.expect("request present in current state")
    }

    fn all_headers(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)> {
        let headers = self.expect_request().headers();

        self.preheaders
            .iter()
            .filter_map(|v| v.as_ref())
            .map(|v| (&v.0, &v.1))
            .chain(headers.iter())
    }

    fn header_len(&self) -> usize {
        self.all_headers().count()
    }
}

#[derive(Debug, Default)]
#[repr(C)]
struct CallState {
    phase: Phase,
    body_mode: BodyMode,
    body_pos: usize,
    request_finished: bool,
    body_reader: Option<BodyReader>,
}

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    SendLine,
    SendHeaders(usize),
    SendBody,
    RecvResponse,
    RecvBody,
}

impl Phase {
    fn is_body(&self) -> bool {
        matches!(self, Phase::SendBody)
    }
}

impl<'a, B> Call<'a, B> {
    pub(crate) fn new(
        request: &'a Request<()>,
        default_body_mode: BodyMode,
    ) -> Result<Self, Error> {
        let info = request.analyze(default_body_mode)?;

        const HEADER_IDX_HOST: usize = 0;
        const HEADER_IDX_BODY: usize = 1;

        let mut preheaders = [None, None];

        if !info.req_host_header {
            if let Some(host) = request.uri().host() {
                // User did not set a host header, and there is one in uri, we set that.
                preheaders[HEADER_IDX_HOST] = Some((
                    HeaderName::from_static("host"),
                    HeaderValue::from_str(host).expect("url host to be valid header value"),
                ));
            }
        }

        if !info.req_body_header && info.body_mode.has_body() {
            // User did not set a body header, we set one.
            let value = match info.body_mode {
                BodyMode::None => unreachable!(),
                BodyMode::Sized(size) => (
                    HeaderName::from_static("content-length"),
                    // TODO(martin): avoid allocation here
                    HeaderValue::from_str(&size.to_string()).unwrap(),
                ),
                BodyMode::Chunked => (
                    HeaderName::from_static("transfer-encoding"),
                    HeaderValue::from_static("chunked"),
                ),
            };
            preheaders[HEADER_IDX_BODY] = Some(value);
        }

        Ok(Call {
            holder: Holder {
                preheaders,
                request: Some(request),
                method: request.method().clone(),
            },
            call_state: CallState {
                body_mode: info.body_mode,
                ..Default::default()
            },
            _ph: PhantomData,
        })
    }

    fn do_into_receive<'b>(self) -> Result<Call<'b, RecvResponse>, Error> {
        if !self.call_state.request_finished {
            return Err(Error::UnfinishedRequest);
        }

        Ok(Call {
            holder: Holder {
                preheaders: self.holder.preheaders,
                request: None,
                method: self.holder.method,
            },
            call_state: CallState {
                phase: Phase::RecvResponse,
                ..self.call_state
            },
            _ph: PhantomData,
        })
    }
}

impl<'a> Call<'a, WithoutBody> {
    /// Write the request to the output buffer
    ///
    /// Returns how much of the output buffer that was used.
    ///
    /// ```
    /// # use hoot::client::Call;
    /// # use hoot::http::Request;
    /// let req = Request::head("http://foo.test/page").body(()).unwrap();
    /// let mut call = Call::without_body(&req).unwrap();
    ///
    /// let mut output = vec![0; 1024];
    /// let n = call.write(&mut output).unwrap();
    /// let s = std::str::from_utf8(&output[..n]).unwrap();
    ///
    /// assert_eq!(s, "HEAD /page HTTP/1.1\r\nhost: foo.test\r\n");
    /// ```
    pub fn write(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        let (_, output_used) = do_write(&self.holder, &mut self.call_state, &[], output, true)?;

        Ok(output_used)
    }

    /// Checks if the request if finished
    ///
    /// Until [`Call::write`] is called enough times (or with a sufficiently large buffer), the
    /// request is not finished.
    pub fn request_finished(&self) -> bool {
        self.call_state.request_finished
    }

    /// Proceed to receiving a response
    ///
    /// Once the request is finished writing, proceed to receiving a response. Will error
    /// if [`Call::request_finished()`] returns `false`.
    pub fn into_receive<'b>(self) -> Result<Call<'b, RecvResponse>, Error> {
        self.do_into_receive()
    }
}

impl<'a> Call<'a, WithBody> {
    /// Write a request, and consecutive body to the output buffer
    ///
    /// The first argument `input` is the body input buffer. If the request contained
    /// a `content-length` header, this buffer must be smaller or same size as that
    /// `content-length`.
    ///
    /// When doing `transfer-encoding: chunked` signal the end of the body by
    /// providing the input `&[]`.
    ///
    /// Returns `(usize, usize)` where the first number is how many bytes of the `input` that
    /// were consumed. The second number is how many bytes of the `output` that were used.
    ///
    /// ```
    /// # use hoot::client::Call;
    /// # use hoot::http::Request;
    /// let req = Request::post("http://f.test/page")
    ///     .header("transfer-encoding", "chunked")
    ///     .body(())
    ///     .unwrap();
    /// let mut call = Call::with_body(&req).unwrap();
    ///
    /// let body = b"hallo";
    ///
    /// // Send body
    /// let mut output = vec![0; 1024];
    /// let (i, n1) = call.write(body, &mut output).unwrap();
    /// assert_eq!(i, 5);
    ///
    /// // Indicate the body is finished by sending &[]
    /// let (_, n2) = call.write(&[], &mut output[n1..]).unwrap();
    /// let s = std::str::from_utf8(&output[..n1 + n2]).unwrap();
    ///
    /// assert_eq!(
    ///     s,
    ///     "POST /page HTTP/1.1\r\nhost: f.test\r\n\
    ///     transfer-encoding: chunked\r\n5\r\nhallo\r\n0\r\n\r\n"
    /// );
    /// ```
    pub fn write(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        if !input.is_empty() {
            if self.call_state.request_finished {
                return Err(Error::BodyContentAfterFinish);
            }

            if let BodyMode::Sized(left) = self.call_state.body_mode {
                if input.len() as u64 > left {
                    return Err(Error::BodyLargerThanContentLength);
                }
            }
        }

        // Sending &[] finishes.
        let fin = match self.call_state.body_mode {
            BodyMode::None => true,
            BodyMode::Sized(left) => input.len() as u64 == left,
            BodyMode::Chunked => input.is_empty(),
        };

        do_write(&self.holder, &mut self.call_state, input, output, fin)
    }

    /// Checks if the request if finished
    ///
    /// Until [`Call::write`] is called with enough input to fulfil the body.
    pub fn request_finished(&self) -> bool {
        self.call_state.request_finished
    }

    /// Proceed to receiving a response
    ///
    /// Once the request is finished writing, proceed to receiving a response. Will error
    /// if [`Call::request_finished()`] returns `false`.
    pub fn into_receive<'b>(self) -> Result<Call<'b, RecvResponse>, Error> {
        self.do_into_receive()
    }
}

fn do_write(
    holder: &Holder<'_>,
    state: &mut CallState,
    input: &[u8],
    output: &mut [u8],
    may_finish: bool,
) -> Result<(usize, usize), Error> {
    if state.request_finished {
        return Ok((0, 0));
    }

    let mut w = Writer::new(output);
    try_write_prelude(holder, state, &mut w)?;

    // When doing chunked, writing a 0 sized input signals the end of
    // the body. The ending is handled below, hence we guard for is_empty() here.
    let input_used = if state.phase.is_body() && !input.is_empty() {
        state.body_mode.write(input, &mut w)
    } else {
        0
    };

    let entire_input_used = input_used == input.len();

    if entire_input_used && may_finish && state.body_mode.finish(&mut w) {
        state.request_finished = true;
    }

    Ok((input_used, w.len()))
}

fn try_write_prelude(
    holder: &Holder<'_>,
    state: &mut CallState,
    w: &mut Writer,
) -> Result<(), Error> {
    let at_start = w.len();

    loop {
        if try_write_prelude_part(holder, state, w) {
            continue;
        }

        let written = w.len() - at_start;

        if written > 0 || state.phase.is_body() {
            return Ok(());
        } else {
            return Err(Error::OutputOverflow);
        }
    }
}

fn try_write_prelude_part(holder: &Holder<'_>, state: &mut CallState, w: &mut Writer) -> bool {
    match &mut state.phase {
        Phase::SendLine => {
            let success = do_write_send_line(holder.expect_request(), w);
            if success {
                state.phase = Phase::SendHeaders(0);
            }
            success
        }

        Phase::SendHeaders(index) => {
            let all = holder.all_headers();
            let skipped = all.skip(*index);

            do_write_headers(skipped, index, w);

            if *index == holder.header_len() {
                state.phase = Phase::SendBody;
            }
            false
        }

        // We're past the header.
        _ => false,
    }
}

fn do_write_send_line<B>(request: &Request<B>, w: &mut Writer) -> bool {
    w.try_write(|w| {
        write!(
            w,
            "{} {} {:?}\r\n",
            request.method(),
            request.uri().path(),
            request.version()
        )
    })
}

fn do_write_headers<'a, I>(headers: I, index: &mut usize, w: &mut Writer)
where
    I: Iterator<Item = (&'a HeaderName, &'a HeaderValue)>,
{
    for h in headers {
        let success = w.try_write(|w| {
            write!(w, "{}: ", h.0)?;
            w.write_all(h.1.as_bytes())?;
            write!(w, "\r\n")?;
            Ok(())
        });

        if success {
            *index += 1;
        } else {
            break;
        }
    }
}

impl<'b> Call<'b, RecvResponse> {
    /// Try reading response headers
    ///
    /// A response is only possible once the `input` holds all the HTTP response
    /// headers. Before that this returns `None`. When the response is succesfully read,
    /// the return value `(usize, Response<()>)` contains how many bytes were consumed
    /// of the `input`.
    ///
    /// Once the response headers are succesfully read, use [`Call::into_body()`] to proceed
    /// reading the response body.
    pub fn try_response(&mut self, input: &[u8]) -> Result<Option<(usize, Response<()>)>, Error> {
        let mut headers = [httparse::EMPTY_HEADER; 50]; // ~1.5k
        let mut res = httparse::Response::new(&mut headers);

        let input_used = match res.parse(input)? {
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

        let http10 = version == Version::HTTP_10;

        let mut builder = Response::builder().version(version).status(status);

        for h in res.headers {
            builder = builder.header(h.name, h.value);
        }

        let response = builder.body(()).expect("a valid response");

        let header_lookup = |name: &str| {
            if let Some(header) = response.headers().get(name) {
                return header.to_str().ok();
            }
            None
        };

        let recv_body_mode =
            BodyReader::for_response(http10, &self.holder.method, status.as_u16(), &header_lookup)?;

        self.call_state.body_reader = Some(recv_body_mode);

        Ok(Some((input_used, response)))
    }

    /// Continue reading the response body
    ///
    /// Errors if called before [`Call::try_response()`] has produced a [`Response`].
    ///
    /// Returns `None` if there is no body such as the response to a `HEAD` request.
    pub fn into_body(self) -> Result<Option<Call<'b, RecvBody>>, Error> {
        let Some(rbm) = &self.call_state.body_reader else {
            return Err(Error::IncompleteResponse);
        };

        // No body is expected either due to Method or status. Call ends here.
        if matches!(rbm, BodyReader::NoBody) {
            return Ok(None);
        }

        Ok(Some(Call {
            holder: Holder {
                preheaders: self.holder.preheaders,
                request: None,
                method: self.holder.method,
            },
            call_state: CallState {
                phase: Phase::RecvBody,
                ..self.call_state
            },
            _ph: PhantomData,
        }))
    }
}

impl<'b> Call<'b, RecvBody> {
    /// Read the input as a response body
    ///
    /// Returns `(usize, usize)` where the first number is how many bytes of the input was used
    /// and the second number how many of the output.
    pub fn read(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        let rbm = self.call_state.body_reader.as_mut().unwrap();

        if rbm.is_ended() {
            return Ok((0, 0));
        }

        rbm.read(input, output)
    }

    /// Tell if the response is over
    pub fn is_ended(&self) -> bool {
        let rbm = self.call_state.body_reader.as_ref().unwrap();
        rbm.is_ended()
    }

    /// Tell if response body is closed delimited
    ///
    /// HTTP/1.0 does not have `content-length` to serialize many requests over the same
    /// socket. Instead it uses socket close to determine the body is finished.
    pub fn is_close_delimited(&self) -> bool {
        let rbm = self.call_state.body_reader.as_ref().unwrap();
        matches!(rbm, BodyReader::CloseDelimited)
    }

    // /// Continue to reading trailer headers
    // pub fn into_trailer(self) -> Result<Option<Call<'b, Trailer>>, Error> {
    //     todo!()
    // }
}

// pub struct Trailer(());

impl<'a, B> fmt::Debug for Call<'a, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Call")
            .field("phase", &self.call_state.phase)
            .finish()
    }
}

impl fmt::Debug for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SendLine => write!(f, "SendLine"),
            Self::SendHeaders(_) => write!(f, "SendHeaders"),
            Self::SendBody => write!(f, "SendBody"),
            Self::RecvResponse => write!(f, "RecvResponse"),
            Self::RecvBody => write!(f, "RecvBody"),
        }
    }
}
