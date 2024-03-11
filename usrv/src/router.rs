use std::marker::PhantomData;

use crate::handler::Handler;
use crate::{Request, Response};

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
    fn get<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self>;

    fn finish(self) -> Service<S, Self> {
        Service {
            _state: PhantomData,
            parent: self,
        }
    }
}

trait Callable<S> {
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
        match self.do_call(state, request) {
            CallResult::Handled(v) => v,
            CallResult::Unhandled(_, _) => Response::not_found(),
        }
    }

    fn do_call(&self, state: S, request: Request) -> CallResult<S> {
        self.parent.call(state, request)
    }
}

impl<S> MethodRouter<S> for Router<S> {
    fn get<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        MethodHandler {
            _htype: PhantomData,
            _state: PhantomData,
            parent: self,
            path,
            handler,
        }
    }
}

pub struct MethodHandler<'a, T, S, H, P> {
    _htype: PhantomData<T>,
    _state: PhantomData<S>,
    parent: P,
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
                if request.matches_path(self.path) {
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

impl<'a, T1, S, H1: Handler<T1, S>, P1: Callable<S>> MethodRouter<S>
    for MethodHandler<'a, T1, S, H1, P1>
{
    fn get<T, H: Handler<T, S>>(self, path: &str, handler: H) -> MethodHandler<T, S, H, Self> {
        MethodHandler {
            _htype: PhantomData,
            _state: PhantomData,
            parent: self,
            path,
            handler,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn make_route() {
        //
        struct AppState;

        fn root() {}
        fn req(_r: Request) {}
        fn foo(_s: &mut AppState) {}
        fn bar(_s: &mut AppState, _r: Request) {}

        let app = Router::with_state::<&mut AppState>()
            //
            .get("/", root)
            .get("/req", req)
            .get("/foo", foo)
            .get("/bar", bar)
            .get("free", |_r: Request| {})
            // .get("/bar2", bar2)
            .finish();

        let mut state = AppState;

        let request = Request;

        let _response = app.call(&mut state, request);
    }
}
