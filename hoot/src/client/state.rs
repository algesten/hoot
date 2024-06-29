use std::fmt;
use std::marker::PhantomData;

use http::{HeaderName, HeaderValue, Method, Request, Response, StatusCode, Uri, Version};
use smallvec::SmallVec;

use crate::analyze::{HeaderIterExt, MethodExt, StatusExt};
use crate::parser::try_parse_response;
use crate::Error;

use super::holder::CallHolder;

pub mod state {
    pub struct Prepare(());
    pub struct SendRequest(());
    pub struct Await100(());
    pub struct SendBody(());
    pub struct RecvResponse(());
    pub struct RecvBody(());
    pub struct Redirect(());
    pub struct Cleanup(());
}
use self::state::*;

pub struct State<'a, State> {
    inner: Inner<'a>,
    _ph: PhantomData<State>,
}

struct Inner<'a> {
    call: CallHolder<'a>,
    close_reason: SmallVec<[CloseReason; 4]>,
    should_send_body: bool,
    await_100_continue: bool,
    status: Option<StatusCode>,
    location: Option<HeaderValue>,
}

impl Inner<'_> {
    fn is_redirect(&self) -> bool {
        match self.status {
            // 304 is a redirect code, but it has no location header and
            // thus we don't consider it a redirection.
            Some(v) => v.is_redirection() && v != StatusCode::NOT_MODIFIED,
            None => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Http10,
    ClientConnectionClose,
    ServerConnectionClose,
    Not100Continue,
}

impl<'a, S> State<'a, S> {
    fn wrap(inner: Inner<'a>) -> State<'a, S> {
        State {
            inner,
            _ph: PhantomData,
        }
    }

    fn call(&self) -> &CallHolder<'a> {
        &self.inner.call
    }

    fn call_mut(&mut self) -> &mut CallHolder<'a> {
        &mut self.inner.call
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// PREPARE

impl<'a> State<'a, Prepare> {
    pub fn new(request: &'a Request<()>) -> Result<Self, Error> {
        let call = CallHolder::new(request)?;

        let mut close_reason = SmallVec::new();

        if request.version() == Version::HTTP_10 {
            // request.analyze() in CallHolder::new() ensures the only versions are HTTP 1.0 and 1.1
            close_reason.push(CloseReason::Http10)
        }

        if request.headers().iter().has("connection", "close") {
            close_reason.push(CloseReason::ClientConnectionClose);
        }

        let should_send_body = request.method().need_request_body();
        let await_100_continue = request.headers().iter().has_expect_100();

        let inner = Inner {
            call,
            close_reason,
            should_send_body,
            await_100_continue,
            status: None,
            location: None,
        };

        Ok(State::wrap(inner))
    }

    pub fn uri(&self) -> &Uri {
        self.call().request().uri()
    }

    pub fn header<K, V>(&mut self, key: K, value: V) -> Result<(), Error>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.call_mut().request_mut().set_header(key, value)
    }

    pub fn proceed(self) -> State<'a, SendRequest> {
        State::wrap(self.inner)
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// SEND REQUEST

impl<'a> State<'a, SendRequest> {
    pub fn write(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        match &mut self.inner.call {
            CallHolder::WithoutBody(v) => v.write(output),
            CallHolder::WithBody(v) => v.write(&[], output).map(|r| r.1),
            _ => unreachable!(),
        }
    }

    pub fn can_proceed(&self) -> bool {
        match &self.inner.call {
            CallHolder::WithoutBody(v) => v.request_finished(),
            CallHolder::WithBody(v) => v.is_body(),
            _ => unreachable!(),
        }
    }

    pub fn proceed(self) -> SendRequestResult<'a> {
        assert!(self.can_proceed(), "SendRequest cannot proceed");

        if self.inner.should_send_body {
            if self.inner.await_100_continue {
                SendRequestResult::Await100(State::wrap(self.inner))
            } else {
                SendRequestResult::SendBody(State::wrap(self.inner))
            }
        } else {
            SendRequestResult::RecvResponse(State::wrap(self.inner))
        }
    }
}

pub enum SendRequestResult<'a> {
    Await100(State<'a, Await100>),
    SendBody(State<'a, SendBody>),
    RecvResponse(State<'a, RecvResponse>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// AWAIT 100

impl<'a> State<'a, Await100> {
    pub fn try_read_100(&mut self, input: &[u8]) -> Result<Option<usize>, Error> {
        // Try parsing a status line without any headers. The line we are looking for is:
        //
        //   HTTP/1.1 100 Continue\r\n\r\n
        //
        // There should be no headers.
        match try_parse_response(input, &mut []) {
            Ok(v) => match v {
                Some((input_used, response)) => {
                    self.inner.await_100_continue = false;

                    if response.status() == StatusCode::CONTINUE {
                        // should_send_body ought to be true since initialization.
                        assert!(self.inner.should_send_body);
                        Ok(Some(input_used))
                    } else {
                        // We encountered a status line, without headers, but it wasn't 100,
                        // so we should not continue to send the body. Furthermore we mustn't
                        // reuse the connection.
                        // https://curl.se/mail/lib-2004-08/0002.html
                        self.inner.close_reason.push(CloseReason::Not100Continue);
                        self.inner.should_send_body = false;
                        Ok(None)
                    }
                }
                // Not enough input yet.
                None => Ok(None),
            },
            Err(e) => {
                if e == Error::HttpParseTooManyHeaders {
                    // We encountered headers after the status line. That means the server did
                    // not send 100-continue, and also continued to produce an answer before we
                    // sent the body. Regardless of what the answer is, we must not send the body.
                    // A 200-answer would be nonsensical given we haven't yet sent the body.
                    //
                    // We do however want to receive the response to be able to provide
                    // the Response<()> to the user. Hence this is not considered an error.
                    self.inner.close_reason.push(CloseReason::Not100Continue);
                    self.inner.should_send_body = false;
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn can_keep_await_100(&self) -> bool {
        self.inner.await_100_continue
    }

    pub fn proceed(self) -> Await100Result<'a> {
        // We can always proceed out of Await100

        if self.inner.should_send_body {
            Await100Result::SendBody(State::wrap(self.inner))
        } else {
            Await100Result::RecvResponse(State::wrap(self.inner))
        }
    }
}

pub enum Await100Result<'a> {
    SendBody(State<'a, SendBody>),
    RecvResponse(State<'a, RecvResponse>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// SEND BODY

impl<'a> State<'a, SendBody> {
    pub fn write(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        self.inner.call.as_with_body_mut().write(input, output)
    }

    pub fn can_proceed(&self) -> bool {
        self.inner.call.as_with_body().request_finished()
    }

    pub fn proceed(mut self) -> State<'a, RecvResponse> {
        assert!(self.can_proceed(), "SendBody cannot proceed");

        let call_body = match self.inner.call {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        };

        // unwrap here is ok because self.can_proceed() should check the necessary
        // error conditions that would prevent us from converting.
        let call_recv = call_body.into_receive().unwrap();

        let call = CallHolder::RecvResponse(call_recv);
        self.inner.call = call;

        State::wrap(self.inner)
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// RECV RESPONSE

impl<'a> State<'a, RecvResponse> {
    pub fn try_response(&mut self, input: &[u8]) -> Result<(usize, Option<Response<()>>), Error> {
        let maybe_response = self.inner.call.as_recv_response_mut().try_response(input)?;

        let (input_used, response) = match maybe_response {
            Some(v) => v,
            // Not enough input for a full response yet
            None => return Ok((0, None)),
        };

        if response.status() == StatusCode::CONTINUE && self.inner.await_100_continue {
            // We have received a "delayed" 100-continue. This means the server did
            // not produce the 100-continue response in time while we were in the
            // state Await100. This is not an error, it can happen if the network is slow.
            self.inner.await_100_continue = false;

            // There should be no headers for this response.
            if !response.headers().is_empty() {
                return Err(Error::HeadersWith100);
            }

            // We should consume the response and wait for the next.
            return Ok((input_used, None));
        }

        self.inner.status = Some(response.status());
        self.inner.location = response.headers().get("location").cloned();

        if response.headers().iter().has("connection", "close") {
            self.inner
                .close_reason
                .push(CloseReason::ServerConnectionClose);
        }

        Ok((input_used, Some(response)))
    }

    pub fn proceed(mut self) -> RecvResponseResult<'a> {
        let call_body = match self.inner.call {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        };

        // unwrap here is ok because it's a fault to call proceed() before
        // try_response() returns Some((x, Response))
        let maybe_call_body = call_body.into_body().unwrap();

        if let Some(call_body) = maybe_call_body {
            self.inner.call = CallHolder::RecvBody(call_body);
            RecvResponseResult::RecvBody(State::wrap(self.inner))
        } else {
            let call_empty = CallHolder::Empty;
            self.inner.call = call_empty;

            if self.inner.is_redirect() {
                RecvResponseResult::Redirect(State::wrap(self.inner))
            } else {
                RecvResponseResult::Cleanup(State::wrap(self.inner))
            }
        }
    }
}

pub enum RecvResponseResult<'a> {
    RecvBody(State<'a, RecvBody>),
    Redirect(State<'a, Redirect>),
    Cleanup(State<'a, Cleanup>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// RECV BODY

impl<'a> State<'a, RecvBody> {
    pub fn read(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        self.inner.call.as_recv_body_mut().read(input, output)
    }

    pub fn can_proceed(&self) -> bool {
        self.inner.call.as_recv_body().is_ended()
    }

    pub fn proceed(self) -> RecvBodyResult<'a> {
        assert!(self.can_proceed(), "RecvBody cannot proceed");

        if self.inner.is_redirect() {
            RecvBodyResult::Redirect(State::wrap(self.inner))
        } else {
            RecvBodyResult::Cleanup(State::wrap(self.inner))
        }
    }
}

pub enum RecvBodyResult<'a> {
    Redirect(State<'a, Redirect>),
    Cleanup(State<'a, Cleanup>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// REDIRECT

impl<'a> State<'a, Redirect> {
    pub fn as_new_state(&self) -> Result<State<'a, Prepare>, Error> {
        let header = match &self.inner.location {
            Some(v) => v,
            None => return Err(Error::NoLocationHeader),
        };

        let location = match header.to_str() {
            Ok(v) => v,
            Err(_) => return Err(Error::BadLocationHeader),
        };

        // Previous request
        let previous = self.inner.call.request();

        // Unwrap is OK, because we can't be here without having read a response.
        let status = self.inner.status.unwrap();
        let method = previous.method();

        // A new uri by combining the base from the previous request and the new location.
        let uri = previous.new_uri_from_location(location)?;

        // Perform the redirect method differently depending on 3xx code.
        let new_method = if status.is_redirect_retaining_method() {
            if method.need_request_body() {
                // only resend the request if it cannot have a body
                return Err(Error::IllegalRedirectSendBody);
            } else if method == Method::DELETE {
                // NOTE: DELETE is intentionally excluded: https://stackoverflow.com/questions/299628
                return Err(Error::IllegalRedirectDelete);
            } else {
                method.clone()
            }
        } else {
            // this is to follow how curl does it. POST, PUT etc change
            // to GET on a redirect.
            if matches!(*method, Method::GET | Method::HEAD) {
                method.clone()
            } else {
                Method::GET
            }
        };

        // Next state
        let mut next = State::new(previous.inner())?;

        let request = next.inner.call.request_mut();

        // Override with the new uri
        request.set_uri(uri);
        request.set_method(new_method);

        Ok(next)
    }

    pub fn proceed(self) -> State<'a, Cleanup> {
        State::wrap(self.inner)
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// CLEANUP

impl<'a> State<'a, Cleanup> {
    pub fn must_close_connection(&self) -> bool {
        let maybe_reason = self.inner.close_reason.first();

        if let Some(reason) = maybe_reason {
            debug!("Close connection because {}", reason);
            true
        } else {
            false
        }
    }
}

// ////////////////////////////////////////////////////////////////////////////////////////////

impl fmt::Display for CloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CloseReason::Http10 => "version is http1.0",
                CloseReason::ClientConnectionClose => "client sent Connection: close",
                CloseReason::ServerConnectionClose => "server sent Connection: close",
                CloseReason::Not100Continue => "got non-100 response before sending body",
            }
        )
    }
}
