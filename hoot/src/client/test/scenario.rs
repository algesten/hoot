use std::marker::PhantomData;

use http::Request;

use crate::client::state::state::{Prepare, RecvResponse, SendBody, SendRequest};
use crate::client::state::{Await100Result, SendRequestResult};
use crate::client::State;

pub struct Scenario<B> {
    request: Request<()>,
    headers_amend: Vec<(String, String)>,
    body: B,
}

impl Scenario<()> {
    pub fn builder() -> ScenarioBuilder<()> {
        ScenarioBuilder::new()
    }
}

impl<B: AsRef<[u8]>> Scenario<B> {
    pub fn to_prepare(&self) -> State<Prepare> {
        // The unwraps here are ok because the user is not supposed to
        // construct tests that test the Scenario builder itself.
        let mut state = State::new(&self.request).unwrap();

        for (key, value) in &self.headers_amend {
            state.header(key, value).unwrap();
        }

        state
    }

    pub fn to_send_request(&self) -> State<SendRequest> {
        let state = self.to_prepare();

        let state = state.proceed();

        state
    }

    pub fn to_send_body(&self) -> State<SendBody> {
        let mut state = self.to_send_request();

        // Write the prelude and discard
        state.write(&mut vec![0; 1024]).unwrap();

        match state.proceed() {
            SendRequestResult::SendBody(v) => v,
            _ => unreachable!("Incorrect scenario not leading to_send_body()"),
        }
    }

    pub fn to_recv_response(&self) -> State<RecvResponse> {
        let mut state = self.to_send_request();

        // Write the prelude and discard
        state.write(&mut vec![0; 1024]).unwrap();

        if state.inner().should_send_body {
            let mut state = if state.inner().await_100_continue {
                // Go via Await100
                let state = match state.proceed() {
                    SendRequestResult::Await100(v) => v,
                    _ => unreachable!(),
                };

                // Proceed straight out of Await100
                match state.proceed() {
                    Await100Result::SendBody(v) => v,
                    _ => unreachable!(),
                }
            } else {
                match state.proceed() {
                    SendRequestResult::SendBody(v) => v,
                    _ => unreachable!(),
                }
            };

            let mut input = self.body.as_ref();
            let mut output = vec![0; 1024];

            while !input.is_empty() {
                let (input_used, _) = state.write(input, &mut output).unwrap();
                input = &input[input_used..];
            }

            state.write(&[], &mut output).unwrap();

            state.proceed()
        } else {
            match state.proceed() {
                SendRequestResult::RecvResponse(v) => v,
                _ => unreachable!(),
            }
        }
    }
}

#[derive(Default)]
pub struct ScenarioBuilder<T> {
    request: Request<()>,
    headers_amend: Vec<(String, String)>,
    _ph: PhantomData<T>,
}

pub struct WithReq(());

#[allow(unused)]
impl ScenarioBuilder<()> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn request(self, request: Request<()>) -> ScenarioBuilder<WithReq> {
        ScenarioBuilder {
            request,
            headers_amend: self.headers_amend,
            _ph: PhantomData,
        }
    }

    pub fn get(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::get(uri).body(()).unwrap())
    }

    pub fn head(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::head(uri).body(()).unwrap())
    }

    pub fn post(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::post(uri).body(()).unwrap())
    }

    pub fn options(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::options(uri).body(()).unwrap())
    }

    pub fn delete(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::delete(uri).body(()).unwrap())
    }

    pub fn trace(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::trace(uri).body(()).unwrap())
    }

    pub fn connect(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::connect(uri).body(()).unwrap())
    }

    pub fn patch(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::patch(uri).body(()).unwrap())
    }
}

impl ScenarioBuilder<WithReq> {
    pub fn header(mut self, key: &'static str, value: impl ToString) -> Self {
        self.request
            .headers_mut()
            .append(key, value.to_string().try_into().unwrap());
        self
    }

    pub fn build(self) -> Scenario<[u8; 0]> {
        Scenario {
            request: self.request,
            body: [],
            headers_amend: self.headers_amend,
        }
    }

    pub fn body<B: AsRef<[u8]>>(self, body: B) -> Scenario<B> {
        Scenario {
            request: self.request,
            body,
            headers_amend: self.headers_amend,
        }
    }
}
