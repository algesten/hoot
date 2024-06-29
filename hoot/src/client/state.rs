use std::marker::PhantomData;

use http::{HeaderName, HeaderValue, Request, Uri};

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
    call: CallHolder<'a>,
    _ph: PhantomData<State>,
}

impl<'a, S> State<'a, S> {
    fn do_proceed<S2>(self) -> State<'a, S2> {
        State {
            call: self.call,
            _ph: PhantomData,
        }
    }
}

impl<'a> State<'a, Prepare> {
    pub fn new(request: &'a Request<()>) -> Result<Self, Error> {
        Ok(State {
            call: CallHolder::new(request)?,
            _ph: PhantomData,
        })
    }

    pub fn uri(&self) -> &Uri {
        self.call.amended().uri()
    }

    pub fn header<K, V>(&mut self, key: K, value: V) -> Result<(), Error>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.call.amended_mut().set_header(key, value)
    }

    pub fn proceed(self) -> State<'a, ObtainConnection> {
        self.do_proceed()
    }
}

impl<'a> State<'a, ObtainConnection> {
    pub fn uri(&self) -> &Uri {
        self.call.amended().uri()
    }

    pub fn proceed(self) -> State<'a, SendRequest> {
        self.do_proceed()
    }
}

impl<'a> State<'a, SendRequest> {
    // pub fn write(&mut self, output: &mut &[u8]) -> Result<usize, Error> {

    // }

    pub fn proceed(self) -> SendRequestResult<'a> {
        todo!()
    }
}

pub enum SendRequestResult<'a> {
    Await100(State<'a, Await100>),
    SendBody(State<'a, SendBody>),
}

impl<'a> State<'a, SendBody> {
    pub fn proceed(self) -> State<'a, RecvResponse> {
        todo!()
    }
}

impl<'a> State<'a, RecvResponse> {
    pub fn proceed(self) -> RecvResponseResult<'a> {
        todo!()
    }
}

pub enum RecvResponseResult<'a> {
    RecvBody(State<'a, RecvBody>),
    Redirect(State<'a, Redirect>),
}

impl<'a> State<'a, RecvBody> {
    pub fn proceed(self) -> State<'a, Cleanup> {
        todo!()
    }
}

impl<'a> State<'a, Redirect> {
    pub fn proceed(self) -> State<'a, Cleanup> {
        todo!()
    }
}

impl<'a> State<'a, Cleanup> {
    pub fn proceed(self) -> CleanupResult<'a> {
        todo!()
    }
}

pub enum CleanupResult<'a> {
    ReuseConnection(State<'a, ReuseConnection>),
    CloseConnection(State<'a, CloseConnection>),
}

impl<'a> State<'a, ReuseConnection> {}

impl<'a> State<'a, CloseConnection> {}
