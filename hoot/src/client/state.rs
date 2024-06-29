use std::fmt;
use std::marker::PhantomData;

use http::{HeaderName, HeaderValue, Request, Response, StatusCode, Uri, Version};
use smallvec::SmallVec;

use crate::analyze::{HeaderIterExt, MethodExt};
use crate::Error;

use super::holder::CallHolder;

pub mod state {
    pub struct Prepare(());
    pub struct ObtainConnection(());
    pub struct SendRequest(());
    // https://curl.se/mail/lib-2004-08/0002.html
    pub struct Await100(());
    pub struct SendBody(());
    pub struct RecvResponse(());
    pub struct RecvBody(());
    pub struct Redirect(());
    pub struct Cleanup(());
    pub struct ReuseConnection(());
    pub struct CloseConnection(());
}
use self::state::*;

pub struct State<'a, State> {
    inner: Inner<'a>,
    _ph: PhantomData<State>,
}

struct Inner<'a> {
    call: CallHolder<'a>,
    close_reason: SmallVec<[CloseReason; 4]>,
    status: Option<StatusCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Http10,
    ClientConnectionClose,
    ServerConnectionClose,
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

        let inner = Inner {
            call,
            close_reason,
            status: None,
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
        self.call_mut().request_must().set_header(key, value)
    }

    pub fn proceed(self) -> State<'a, ObtainConnection> {
        State::wrap(self.inner)
    }
}

impl<'a> State<'a, ObtainConnection> {
    pub fn uri(&self) -> &Uri {
        self.call().request().uri()
    }

    pub fn proceed(self) -> State<'a, SendRequest> {
        State::wrap(self.inner)
    }
}

impl<'a> State<'a, SendRequest> {
    pub fn write(&mut self, output: &mut &[u8]) -> Result<usize, Error> {
        todo!()
    }

    pub fn proceed(self) -> SendRequestResult<'a> {
        let request = self.call().request();

        if request.method().need_request_body() {
            if request.is_expect_100() {
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

impl<'a> State<'a, SendBody> {
    pub fn write(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        todo!()
    }

    pub fn proceed(self) -> State<'a, RecvResponse> {
        State::wrap(self.inner)
    }
}

impl<'a> State<'a, RecvResponse> {
    pub fn try_response(&mut self, input: &[u8]) -> Result<Option<(usize, Response<()>)>, Error> {
        let response: Response<()> = todo!();

        self.inner.status = Some(response.status());

        if response.headers().iter().has("connection", "close") {
            self.inner
                .close_reason
                .push(CloseReason::ServerConnectionClose);
        }

        Ok(Some((0, response)))
    }

    pub fn proceed(self) -> RecvResponseResult<'a> {
        if self.inner.status.unwrap().is_redirection() {
            RecvResponseResult::Redirect(State::wrap(self.inner))
        } else {
            RecvResponseResult::RecvBody(State::wrap(self.inner))
        }
    }
}

pub enum RecvResponseResult<'a> {
    RecvBody(State<'a, RecvBody>),
    Redirect(State<'a, Redirect>),
}

impl<'a> State<'a, RecvBody> {
    pub fn read(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        todo!()
    }

    pub fn proceed(self) -> State<'a, Cleanup> {
        State::wrap(self.inner)
    }
}

impl<'a> State<'a, Redirect> {
    pub fn as_new_state(&self) -> State<'a, Prepare> {
        todo!()
    }

    pub fn proceed(self) -> State<'a, Cleanup> {
        State::wrap(self.inner)
    }
}

impl<'a> State<'a, Cleanup> {
    pub fn proceed(self) -> CleanupResult<'a> {
        let reason = self.inner.close_reason.first();

        if let Some(reason) = reason {
            debug!("Close connection because {}", reason);

            CleanupResult::CloseConnection(State::wrap(self.inner))
        } else {
            CleanupResult::ReuseConnection(State::wrap(self.inner))
        }
    }
}

pub enum CleanupResult<'a> {
    ReuseConnection(State<'a, ReuseConnection>),
    CloseConnection(State<'a, CloseConnection>),
}

impl<'a> State<'a, ReuseConnection> {}

impl<'a> State<'a, CloseConnection> {}

impl fmt::Display for CloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                CloseReason::Http10 => "version is http1.0",
                CloseReason::ClientConnectionClose => "client sent Connection: close",
                CloseReason::ServerConnectionClose => "server sent Connection: close",
            }
        )
    }
}
