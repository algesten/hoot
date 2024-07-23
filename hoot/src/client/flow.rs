use std::fmt;
use std::marker::PhantomData;

use http::uri::Scheme;
use http::{
    HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Uri, Version,
};
use smallvec::SmallVec;

use crate::ext::{HeaderIterExt, MethodExt, StatusExt};
use crate::parser::try_parse_response;
use crate::{BodyMode, Error};

use super::holder::CallHolder;

pub mod state {
    pub(crate) trait Named {
        fn name() -> &'static str;
    }

    macro_rules! flow_state {
        ($n:tt) => {
            pub struct $n(());
            impl Named for $n {
                fn name() -> &'static str {
                    stringify!($n)
                }
            }
        };
    }

    flow_state!(Prepare);
    flow_state!(SendRequest);
    flow_state!(Await100);
    flow_state!(SendBody);
    flow_state!(RecvResponse);
    flow_state!(RecvBody);
    flow_state!(Redirect);
    flow_state!(Cleanup);
}
use self::state::*;

pub struct Flow<B, State> {
    inner: Inner<B>,
    _ph: PhantomData<State>,
}

// pub(crate) for tests to inspect state
#[derive(Debug)]
pub(crate) struct Inner<B> {
    pub call: CallHolder<B>,
    pub close_reason: SmallVec<[CloseReason; 4]>,
    pub should_send_body: bool,
    pub await_100_continue: bool,
    pub status: Option<StatusCode>,
    pub location: Option<HeaderValue>,
}

impl<B> Inner<B> {
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
    CloseDelimitedBody,
}

impl CloseReason {
    fn explain(&self) -> &'static str {
        match self {
            CloseReason::Http10 => "version is http1.0",
            CloseReason::ClientConnectionClose => "client sent Connection: close",
            CloseReason::ServerConnectionClose => "server sent Connection: close",
            CloseReason::Not100Continue => "got non-100 response before sending body",
            CloseReason::CloseDelimitedBody => "response body is close delimited",
        }
    }
}

impl<B, S> Flow<B, S> {
    fn wrap(inner: Inner<B>) -> Flow<B, S> {
        Flow {
            inner,
            _ph: PhantomData,
        }
    }

    fn call(&self) -> &CallHolder<B> {
        &self.inner.call
    }

    fn call_mut(&mut self) -> &mut CallHolder<B> {
        &mut self.inner.call
    }

    #[cfg(test)]
    pub(crate) fn inner(&self) -> &Inner<B> {
        &self.inner
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// PREPARE

impl<B> Flow<B, Prepare> {
    pub fn new(request: Request<B>) -> Result<Self, Error> {
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

        let call = CallHolder::new(request)?;

        let inner = Inner {
            call,
            close_reason,
            should_send_body,
            await_100_continue,
            status: None,
            location: None,
        };

        Ok(Flow::wrap(inner))
    }

    pub fn method(&self) -> &Method {
        self.call().request().method()
    }

    pub fn uri(&self) -> &Uri {
        self.call().request().uri()
    }

    pub fn version(&self) -> Version {
        self.call().request().version()
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

    pub fn proceed(self) -> Flow<B, SendRequest> {
        Flow::wrap(self.inner)
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// SEND REQUEST

impl<B> Flow<B, SendRequest> {
    pub fn write(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        match &mut self.inner.call {
            CallHolder::WithoutBody(v) => v.write(output),
            CallHolder::WithBody(v) => v.write(&[], output).map(|r| r.1),
            _ => unreachable!(),
        }
    }

    pub fn method(&self) -> &Method {
        self.call().request().method()
    }

    pub fn uri(&self) -> &Uri {
        self.call().request().uri()
    }

    pub fn version(&self) -> Version {
        self.call().request().version()
    }

    pub fn headers_map(&mut self) -> Result<HeaderMap, Error> {
        self.call_mut().analyze_request()?;
        let mut map = HeaderMap::new();
        for (k, v) in self.call().request().headers() {
            map.insert(k, v.clone());
        }
        Ok(map)
    }

    pub fn can_proceed(&self) -> bool {
        match &self.inner.call {
            CallHolder::WithoutBody(v) => v.is_finished(),
            CallHolder::WithBody(v) => v.is_body(),
            _ => unreachable!(),
        }
    }

    pub fn proceed(mut self) -> Option<SendRequestResult<B>> {
        if !self.can_proceed() {
            return None;
        }

        if self.inner.should_send_body {
            Some(if self.inner.await_100_continue {
                SendRequestResult::Await100(Flow::wrap(self.inner))
            } else {
                SendRequestResult::SendBody(Flow::wrap(self.inner))
            })
        } else {
            let call = match self.inner.call {
                CallHolder::WithoutBody(v) => v,
                _ => unreachable!(),
            };

            // unwrap here is ok because self.can_proceed() should check the necessary
            // error conditions that would prevent us from converting.
            let call_recv = call.into_receive().unwrap();

            let call = CallHolder::RecvResponse(call_recv);
            self.inner.call = call;

            Some(SendRequestResult::RecvResponse(Flow::wrap(self.inner)))
        }
    }
}

pub enum SendRequestResult<B> {
    Await100(Flow<B, Await100>),
    SendBody(Flow<B, SendBody>),
    RecvResponse(Flow<B, RecvResponse>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// AWAIT 100

impl<B> Flow<B, Await100> {
    pub fn try_read_100(&mut self, input: &[u8]) -> Result<usize, Error> {
        // Try parsing a status line without any headers. The line we are looking for is:
        //
        //   HTTP/1.1 100 Continue\r\n\r\n
        //
        // There should be no headers.
        match try_parse_response::<0>(input) {
            Ok(v) => match v {
                Some((input_used, response)) => {
                    self.inner.await_100_continue = false;

                    if response.status() == StatusCode::CONTINUE {
                        // should_send_body ought to be true since initialization.
                        assert!(self.inner.should_send_body);
                        Ok(input_used)
                    } else {
                        // We encountered a status line, without headers, but it wasn't 100,
                        // so we should not continue to send the body. Furthermore we mustn't
                        // reuse the connection.
                        // https://curl.se/mail/lib-2004-08/0002.html
                        self.inner.close_reason.push(CloseReason::Not100Continue);
                        self.inner.should_send_body = false;
                        Ok(0)
                    }
                }
                // Not enough input yet.
                None => Ok(0),
            },
            Err(e) => {
                self.inner.await_100_continue = false;

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
                    Ok(0)
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn can_keep_await_100(&self) -> bool {
        self.inner.await_100_continue
    }

    pub fn proceed(self) -> Await100Result<B> {
        // We can always proceed out of Await100

        if self.inner.should_send_body {
            Await100Result::SendBody(Flow::wrap(self.inner))
        } else {
            Await100Result::RecvResponse(Flow::wrap(self.inner))
        }
    }
}

pub enum Await100Result<B> {
    SendBody(Flow<B, SendBody>),
    RecvResponse(Flow<B, RecvResponse>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// SEND BODY

impl<B> Flow<B, SendBody> {
    pub fn write(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        self.inner.call.as_with_body_mut().write(input, output)
    }

    pub fn consume_direct_write(&mut self, amount: usize) -> Result<(), Error> {
        self.inner
            .call
            .as_with_body_mut()
            .consume_direct_write(amount)
    }

    pub fn calculate_output_overhead(&mut self, output_len: usize) -> Result<usize, Error> {
        let call = self.inner.call.as_with_body_mut();
        call.analyze_request()?;

        Ok(if call.is_chunked() {
            // The + 1 and floor() is to make even powers of 16 right.
            // The + 4 is for the \r\n overhead. A chunk is:
            // <digits_in_hex>\r\n
            // <chunk>\r\n
            // 0\r\n
            // \r\n
            ((output_len as f64).log(16.0) + 1.0).floor() as usize + 4
        } else {
            0
        })
    }

    pub fn can_proceed(&self) -> bool {
        self.inner.call.as_with_body().is_finished()
    }

    pub fn proceed(mut self) -> Option<Flow<B, RecvResponse>> {
        if !self.can_proceed() {
            return None;
        }

        let call_body = match self.inner.call {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        };

        // unwrap here is ok because self.can_proceed() should check the necessary
        // error conditions that would prevent us from converting.
        let call_recv = call_body.into_receive().unwrap();

        let call = CallHolder::RecvResponse(call_recv);
        self.inner.call = call;

        Some(Flow::wrap(self.inner))
    }
}

// //////////////////////////////////////////////////////////////////////////////////////////// RECV RESPONSE

impl<B> Flow<B, RecvResponse> {
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

            // We should consume the response and wait for the next.
            return Ok((input_used, None));
        }

        self.inner.status = Some(response.status());
        // We want the last Location header.
        self.inner.location = response
            .headers()
            .get_all("location")
            .into_iter()
            .last()
            .cloned();

        if response.headers().iter().has("connection", "close") {
            self.inner
                .close_reason
                .push(CloseReason::ServerConnectionClose);
        }

        Ok((input_used, Some(response)))
    }

    pub fn can_proceed(&self) -> bool {
        self.inner.call.as_recv_response().is_finished()
    }

    pub fn proceed(mut self) -> Option<RecvResponseResult<B>> {
        if !self.can_proceed() {
            return None;
        }

        let call_body = match self.inner.call {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        };

        let has_response_body = call_body.need_response_body();
        let call_body = call_body.do_into_body();

        if has_response_body {
            if call_body.is_close_delimited() {
                self.inner
                    .close_reason
                    .push(CloseReason::CloseDelimitedBody);
            }

            self.inner.call = CallHolder::RecvBody(call_body);

            Some(RecvResponseResult::RecvBody(Flow::wrap(self.inner)))
        } else {
            self.inner.call = CallHolder::RecvBody(call_body);

            Some(if self.inner.is_redirect() {
                RecvResponseResult::Redirect(Flow::wrap(self.inner))
            } else {
                RecvResponseResult::Cleanup(Flow::wrap(self.inner))
            })
        }
    }
}

pub enum RecvResponseResult<B> {
    RecvBody(Flow<B, RecvBody>),
    Redirect(Flow<B, Redirect>),
    Cleanup(Flow<B, Cleanup>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// RECV BODY

impl<B> Flow<B, RecvBody> {
    pub fn read(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        self.inner.call.as_recv_body_mut().read(input, output)
    }

    pub fn can_proceed(&self) -> bool {
        let call = self.inner.call.as_recv_body();
        call.is_ended() || call.is_close_delimited()
    }

    pub fn body_mode(&self) -> BodyMode {
        self.call().body_mode()
    }

    pub fn proceed(self) -> Option<RecvBodyResult<B>> {
        if !self.can_proceed() {
            return None;
        }

        Some(if self.inner.is_redirect() {
            RecvBodyResult::Redirect(Flow::wrap(self.inner))
        } else {
            RecvBodyResult::Cleanup(Flow::wrap(self.inner))
        })
    }
}

pub enum RecvBodyResult<B> {
    Redirect(Flow<B, Redirect>),
    Cleanup(Flow<B, Cleanup>),
}

// //////////////////////////////////////////////////////////////////////////////////////////// REDIRECT

impl<B> Flow<B, Redirect> {
    pub fn as_new_flow(
        &mut self,
        redirect_auth_headers: RedirectAuthHeaders,
    ) -> Result<Option<Flow<B, Prepare>>, Error> {
        let header = match &self.inner.location {
            Some(v) => v,
            None => return Err(Error::NoLocationHeader),
        };

        let location = match header.to_str() {
            Ok(v) => v,
            Err(_) => {
                return Err(Error::BadLocationHeader(
                    String::from_utf8_lossy(header.as_bytes()).to_string(),
                ))
            }
        };

        // Previous request
        let previous = self.inner.call.request_mut();

        // Unwrap is OK, because we can't be here without having read a response.
        let status = self.inner.status.unwrap();
        let method = previous.method();

        // A new uri by combining the base from the previous request and the new location.
        let uri = previous.new_uri_from_location(location)?;

        // Perform the redirect method differently depending on 3xx code.
        let new_method = if status.is_redirect_retaining_status() {
            if method.need_request_body() {
                // only resend the request if it cannot have a body
                return Ok(None);
            } else if method == Method::DELETE {
                // NOTE: DELETE is intentionally excluded: https://stackoverflow.com/questions/299628
                return Ok(None);
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

        let mut request = previous.take_request();
        *request.method_mut() = new_method;

        // Next state
        let mut next = Flow::new(request)?;

        let request = next.inner.call.request_mut();

        let keep_auth_header = match redirect_auth_headers {
            RedirectAuthHeaders::Never => false,
            RedirectAuthHeaders::SameHost => can_redirect_auth_header(request.uri(), &uri),
        };

        // Override with the new uri
        request.set_uri(uri);

        if !keep_auth_header {
            request.unset_header("authorization")?;
        }
        request.unset_header("cookie")?;
        request.unset_header("content-length")?;

        // TODO(martin): clear out unwanted headers

        Ok(Some(next))
    }

    pub fn status(&self) -> StatusCode {
        self.inner.status.unwrap()
    }

    pub fn must_close_connection(&self) -> bool {
        self.close_reason().is_some()
    }

    pub fn close_reason(&self) -> Option<&'static str> {
        self.inner.close_reason.first().map(|s| s.explain())
    }

    pub fn proceed(self) -> Flow<B, Cleanup> {
        Flow::wrap(self.inner)
    }
}

fn can_redirect_auth_header(prev: &Uri, next: &Uri) -> bool {
    let host_prev = prev.authority().map(|a| a.host());
    let host_next = next.authority().map(|a| a.host());
    let scheme_prev = prev.scheme();
    let scheme_next = next.scheme();
    host_prev == host_next && (scheme_prev == scheme_next || scheme_next == Some(&Scheme::HTTPS))
}

/// Strategy for keeping `authorization` headers during redirects.
///
/// * `Never` never preserves `authorization` header in redirects.
/// * `SameHost` send the authorization header in redirects only if the host of the redirect is
/// the same of the previous request, and both use the same scheme (or switch to a more secure one, i.e
/// we can redirect from `http` to `https`, but not the reverse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RedirectAuthHeaders {
    /// Never preserve the `authorization` header on redirect. This is the default.
    Never,
    /// Preserve the `authorization` header when the redirect is to the same host. Both hosts must use
    /// the same scheme (or switch to a more secure one, i.e we can redirect from `http` to `https`,
    /// but not the reverse).
    SameHost,
}

// //////////////////////////////////////////////////////////////////////////////////////////// CLEANUP

impl<B> Flow<B, Cleanup> {
    pub fn must_close_connection(&self) -> bool {
        self.close_reason().is_some()
    }

    pub fn close_reason(&self) -> Option<&'static str> {
        self.inner.close_reason.first().map(|s| s.explain())
    }
}

// ////////////////////////////////////////////////////////////////////////////////////////////

impl<B, State: Named> fmt::Debug for Flow<B, State> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Flow")
            .field("state", &State::name())
            .finish()
    }
}
