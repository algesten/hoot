use std::marker::PhantomData;
use std::{io, thread};

use http::Method;

use crate::handler::Handler;
use crate::read_req::read_from_buffers;
use crate::response::{IntoResponse, NotFound};
use crate::server::Acceptor;
use crate::write_res::write_response_with_buffer;
use crate::{read_request, Error, Request, Response};

pub struct Router<S = ()> {
    _state: PhantomData<S>,
}

impl Router {
    pub fn new() -> Self {
        Self::with_state::<()>()
    }

    pub fn with_state<S>() -> Router<S> {
        Router {
            _state: PhantomData,
        }
    }
}

impl<S> Callable<S> for Router<S> {
    fn call(&self, state: S, request: Request) -> CallResult<S> {
        CallResult::Unhandled(state, request)
    }
}

#[allow(private_bounds)]
pub trait MethodRouter<S>: Sized + Callable<S> {
    fn finish(self) -> Service<S, Self> {
        Service {
            _state: PhantomData,
            parent: self,
        }
    }

    #[doc(hidden)]
    fn handle<T, H: Handler<T, S>>(
        self,
        method: Method,
        path: &str,
        handler: H,
    ) -> MethodHandler<T, S, H, Self>;

    fn get<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::GET, path, handler)
    }

    fn post<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::POST, path, handler)
    }

    fn put<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::PUT, path, handler)
    }

    fn delete<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::DELETE, path, handler)
    }

    fn head<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::HEAD, path, handler)
    }

    fn options<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::OPTIONS, path, handler)
    }

    fn connect<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::CONNECT, path, handler)
    }

    fn patch<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::PATCH, path, handler)
    }

    fn trace<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        Self::handle(self, Method::TRACE, path, handler)
    }
}

trait Callable<S>: Clone {
    fn call(&self, state: S, request: Request) -> CallResult<S>;
}

enum CallResult<S> {
    Handled(Response),
    Unhandled(S, Request),
}

pub struct Service<S, P> {
    _state: PhantomData<S>,
    parent: P,
}

#[allow(private_bounds)]
impl<S, P: Callable<S>> Service<S, P> {
    pub fn call(&self, state: S, request: Request) -> Response {
        match self.parent.call(state, request) {
            CallResult::Handled(v) => v,
            CallResult::Unhandled(_, _) => NotFound.into_response(),
        }
    }

    #[allow(unused)]
    pub fn drive(
        &self,
        state: S,
        reader: impl io::Read + Send + 'static,
        writer: &mut dyn io::Write,
    ) -> Result<(), Error>
    where
        S: Clone,
    {
        let Some(mut request) = read_request(reader)? else {
            return Ok(());
        };

        loop {
            let request_method = request.method().clone();

            // This is a cheap clone using Rc. This is so we can retain the HootBody
            // for consecutive requests. After this line we have two instances of Rc
            // to the same HootBody.
            let body = request.body().hoot_clone();

            // The call consumes the Rc instance in Request<Body>, leaving a single
            // Rc in the "body" var. Body is deliberately !Send, which means the,
            // Handlers nested in the call cannot retain the copy to the Rc<HootBody>.
            let response = self.call(state.clone(), request);

            // This should succeed because there should be only one Rc.
            let hoot_body = body.hoot_unwrap();

            // Get the buffers back to reuse for next request.
            let (mut parse_buf, fill_buf) = hoot_body.into_buffers();

            write_response_with_buffer(request_method, response, writer, &mut parse_buf)?;

            let Some(next_request) = read_from_buffers(parse_buf, fill_buf)? else {
                break;
            };

            request = next_request;
        }

        Ok(())
    }

    pub fn run<A>(&self, state: S, mut acceptor: A) -> Result<(), Error>
    where
        S: Clone + Send + 'static,
        P: Send + 'static,
        A: Acceptor,
    {
        loop {
            let (reader, mut writer, _breaker) = acceptor.accept()?;

            let service = self.clone();
            let state = state.clone();

            thread::spawn(move || {
                if let Err(e) = service.drive(state, reader, &mut writer) {
                    match e {
                        Error::Hoot(e) => error!("service error: {}", e),
                        Error::Io(e) => debug!("client disconnect: {}", e),
                    }
                }
            });
        }
    }

    pub fn execute<A>(&self, state: S, acceptor: &mut A) -> Result<A::Writer, Error>
    where
        S: Clone + 'static,
        P: 'static,
        A: Acceptor,
    {
        let (reader, mut writer, _breaker) = acceptor.accept()?;

        let service = self.clone();
        let state = state.clone();

        service.drive(state, reader, &mut writer)?;

        Ok(writer)
    }
}

impl<S> MethodRouter<S> for Router<S> {
    fn handle<T, H: Handler<T, S>>(
        self,
        method: Method,
        path: &str,
        handler: H,
    ) -> MethodHandler<T, S, H, Self> {
        MethodHandler {
            _htype: PhantomData,
            _state: PhantomData,
            parent: self,
            method,
            path,
            handler,
        }
    }
}

pub struct MethodHandler<'a, T, S, H, P> {
    _htype: PhantomData<T>,
    _state: PhantomData<S>,
    parent: P,
    method: Method,
    path: &'a str,
    handler: H,
}

impl<'a, T, S, H: Handler<T, S>, P: Callable<S>> Callable<S> for MethodHandler<'a, T, S, H, P> {
    fn call(&self, state: S, request: Request) -> CallResult<S> {
        // First call parent since that reflects the order the handlers are declared.
        match self.parent.call(state, request) {
            // Parent handled request, pass response on
            CallResult::Handled(r) => CallResult::Handled(r),

            // Parent did not handle request
            CallResult::Unhandled(state, request) => {
                // Try to match to our path
                if request_matcher(&request, &self.method, self.path) {
                    // Run our handler
                    let result = self.handler.clone().call(state, request);

                    // Result is now handled
                    CallResult::Handled(result)
                } else {
                    // Path doesn't match, we are not to run the handler
                    CallResult::Unhandled(state, request)
                }
            }
        }
    }
}

fn request_matcher(_request: &Request, _method: &Method, _path: &str) -> bool {
    todo!()
}

impl<'a, T1, S, H1: Handler<T1, S>, P1: Callable<S>> MethodRouter<S>
    for MethodHandler<'a, T1, S, H1, P1>
{
    fn handle<T, H: Handler<T, S>>(
        self,
        method: Method,
        path: &str,
        handler: H,
    ) -> MethodHandler<T, S, H, Self> {
        MethodHandler {
            _htype: PhantomData,
            _state: PhantomData,
            parent: self,
            method,
            path,
            handler,
        }
    }
}

impl<S> Clone for Router<S> {
    fn clone(&self) -> Self {
        Self {
            _state: PhantomData,
        }
    }
}

impl<S, P: Clone> Clone for Service<S, P> {
    fn clone(&self) -> Self {
        Self {
            _state: PhantomData,
            parent: self.parent.clone(),
        }
    }
}

impl<'a, T, S, H: Clone, P: Clone> Clone for MethodHandler<'a, T, S, H, P> {
    fn clone(&self) -> Self {
        Self {
            _htype: PhantomData,
            _state: PhantomData,
            parent: self.parent.clone(),
            method: self.method.clone(),
            path: self.path,
            handler: self.handler.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::server::test::TestAcceptor;

    use super::*;

    #[test]
    fn make_route() {
        //
        struct AppState;

        fn root() {}
        fn req(_r: Request) {}
        fn req_x(_s: &mut AppState, _r: Request) -> &str {
            "hello world"
        }
        fn foo(_s: &mut AppState) {}
        fn bar(_s: &mut AppState, _r: Request) {}

        let service = Router::with_state::<&mut AppState>()
            //
            .get("/", root)
            .get("/req", req)
            .get("/req_x", req_x)
            .get("/foo", foo)
            .get("/bar", bar)
            .get("free", |_r: Request| {})
            // .get("/bar2", bar2)
            .finish();

        let mut state = AppState;

        let request = http::Request::get("/").body(().into()).unwrap();

        let cloned = service.clone();

        let _response = cloned.call(&mut state, request);
    }

    #[test]
    fn run_service() {
        #[derive(Clone)]
        struct AppState;

        fn handle(_req: Request) -> String {
            "Hello World!".into()
        }

        let service = Router::with_state::<AppState>()
            //
            .get("/", handle)
            .finish();

        let state = AppState;

        let mut acceptor = TestAcceptor::new(http::Request::get("/").body(()).unwrap());

        let writer = service.execute(state, &mut acceptor).unwrap();
    }
}
