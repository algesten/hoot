use std::io::Write;
use std::marker::PhantomData;

use http::{Request, Response};

use crate::client::flow::state::{
    Await100, Prepare, RecvBody, RecvResponse, SendBody, SendRequest,
};
use crate::client::flow::{Await100Result, SendRequestResult};
use crate::client::results::RecvResponseResult;
use crate::client::Flow;

pub struct Scenario {
    request: Request<()>,
    headers_amend: Vec<(String, String)>,
    response: Response<()>,
    body: Vec<u8>,
}

impl Scenario {
    pub fn builder() -> ScenarioBuilder<()> {
        ScenarioBuilder::new()
    }
}

impl Scenario {
    pub fn to_prepare(&self) -> Flow<Prepare> {
        // The unwraps here are ok because the user is not supposed to
        // construct tests that test the Scenario builder itself.
        let mut flow = Flow::new(&self.request).unwrap();

        for (key, value) in &self.headers_amend {
            flow.header(key, value).unwrap();
        }

        flow
    }

    pub fn to_send_request(&self) -> Flow<SendRequest> {
        let flow = self.to_prepare();

        let flow = flow.proceed();

        flow
    }

    pub fn to_send_body(&self) -> Flow<SendBody> {
        let mut flow = self.to_send_request();

        // Write the prelude and discard
        flow.write(&mut vec![0; 1024]).unwrap();

        match flow.proceed() {
            SendRequestResult::SendBody(v) => v,
            _ => unreachable!("Incorrect scenario not leading to_send_body()"),
        }
    }

    pub fn to_await_100(&self) -> Flow<Await100> {
        let mut flow = self.to_send_request();

        // Write the prelude and discard
        flow.write(&mut vec![0; 1024]).unwrap();

        match flow.proceed() {
            SendRequestResult::Await100(v) => v,
            _ => unreachable!("Incorrect scenario not leading to_await_100()"),
        }
    }

    pub fn to_recv_response(&self) -> Flow<RecvResponse> {
        let mut flow = self.to_send_request();

        // Write the prelude and discard
        flow.write(&mut vec![0; 1024]).unwrap();

        if flow.inner().should_send_body {
            let mut flow = if flow.inner().await_100_continue {
                // Go via Await100
                let flow = match flow.proceed() {
                    SendRequestResult::Await100(v) => v,
                    _ => unreachable!(),
                };

                // Proceed straight out of Await100
                match flow.proceed() {
                    Await100Result::SendBody(v) => v,
                    _ => unreachable!(),
                }
            } else {
                match flow.proceed() {
                    SendRequestResult::SendBody(v) => v,
                    _ => unreachable!(),
                }
            };

            let mut input = &self.body[..];
            let mut output = vec![0; 1024];

            while !input.is_empty() {
                let (input_used, _) = flow.write(input, &mut output).unwrap();
                input = &input[input_used..];
            }

            flow.write(&[], &mut output).unwrap();

            flow.proceed()
        } else {
            match flow.proceed() {
                SendRequestResult::RecvResponse(v) => v,
                _ => unreachable!(),
            }
        }
    }

    pub fn to_recv_body(&self) -> Flow<RecvBody> {
        let mut state = self.to_recv_response();

        let mut input = Vec::<u8>::new();

        let r = &self.response;
        let s = r.status();

        write!(
            &mut input,
            "{:?} {} {}\r\n",
            r.version(),
            s.as_u16(),
            s.canonical_reason().unwrap()
        )
        .unwrap();

        for (k, v) in r.headers().iter() {
            write!(&mut input, "{}: {}\r\n", k.as_str(), v.to_str().unwrap()).unwrap();
        }

        write!(&mut input, "\r\n").unwrap();

        // use crate::client::test::TestSliceExt;
        // println!("{:?}", input.as_slice().as_str());

        state.try_response(&input).unwrap();

        match state.proceed() {
            RecvResponseResult::RecvBody(v) => v,
            _ => unreachable!("Incorrect scenario not leading to_recv_body()"),
        }
    }
}

#[derive(Default)]
pub struct ScenarioBuilder<T> {
    request: Request<()>,
    headers_amend: Vec<(String, String)>,
    body: Vec<u8>,
    response: Response<()>,
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
            body: vec![],
            response: Response::default(),
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

    pub fn put(self, uri: &str) -> ScenarioBuilder<WithReq> {
        self.request(Request::put(uri).body(()).unwrap())
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

#[allow(unused)]
impl ScenarioBuilder<WithReq> {
    pub fn header(mut self, key: &'static str, value: impl ToString) -> Self {
        self.request
            .headers_mut()
            .append(key, value.to_string().try_into().unwrap());
        self
    }

    pub fn body<B: AsRef<[u8]>>(mut self, body: B, chunked: bool) -> Self {
        self.body = body.as_ref().to_vec();

        let (k, v) = if chunked {
            ("transfer-encoding".to_string(), "chunked".to_string())
        } else {
            ("content-length".to_string(), self.body.len().to_string())
        };

        self.headers_amend.push((k, v));

        self
    }

    pub fn response(mut self, response: Response<()>) -> Self {
        self.response = response;
        self
    }

    pub fn build(self) -> Scenario {
        Scenario {
            request: self.request,
            body: self.body,
            headers_amend: self.headers_amend,
            response: self.response,
        }
    }
}
