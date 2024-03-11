use std::convert::Infallible;
use std::marker::PhantomData;

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

pub struct Request;
impl Request {
    fn matches_path(&self, path: &str) -> bool {
        todo!()
    }
}

pub struct Response;

impl Response {
    pub fn not_found() -> Self {
        todo!()
    }
}

impl From<()> for Response {
    fn from(_: ()) -> Self {
        todo!()
    }
}

impl From<Infallible> for Response {
    fn from(_value: Infallible) -> Self {
        panic!("Attempt to convert Infallible to Response")
    }
}

pub trait Handler<T, S>: Clone + Send + Sized + 'static {
    fn call(self, state: S, request: Request) -> Response;
}

impl<S, F, Ret> Handler<(), S> for F
where
    F: FnOnce() -> Ret + Clone + Send + 'static,
    Ret: Into<Response>,
{
    fn call(self, _state: S, _request: Request) -> Response {
        (self)().into()
    }
}

impl<S, F, Ret> Handler<((),), S> for F
where
    F: FnOnce(S) -> Ret + Clone + Send + 'static,
    Ret: Into<Response>,
{
    fn call(self, state: S, _request: Request) -> Response {
        (self)(state).into()
    }
}

impl<S, F, T1, Ret> Handler<((), T1), S> for F
where
    F: FnOnce(T1) -> Ret + Clone + Send + 'static,
    T1: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(t1).into()
    }
}

impl<S, F, T1, Ret> Handler<(((),), T1), S> for F
where
    F: FnOnce(S, T1) -> Ret + Clone + Send + 'static,
    T1: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(state, t1).into()
    }
}

impl<S, F, T1, T2, Ret> Handler<((), T1, T2), S> for F
where
    F: FnOnce(T1, T2) -> Ret + Clone + Send + 'static,
    T1: FromRequestRef<S>,
    T2: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, &request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        let t2 = match T2::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(t1, t2).into()
    }
}

impl<S, F, T1, T2, Ret> Handler<(((),), T1, T2), S> for F
where
    F: FnOnce(S, T1, T2) -> Ret + Clone + Send + 'static,
    T1: FromRequestRef<S>,
    T2: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, &request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        let t2 = match T2::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(state, t1, t2).into()
    }
}

pub trait FromRequest<S>: Sized {
    type Rejection: Into<Response>;
    fn from_request(state: &S, request: Request) -> Result<Self, Self::Rejection>;
}

impl<S> FromRequest<S> for Request {
    type Rejection = Infallible;

    fn from_request(_state: &S, request: Request) -> Result<Self, Self::Rejection> {
        Ok(request)
    }
}

pub trait FromRequestRef<S>: Sized {
    type Rejection: Into<Response>;
    fn from_request<'a, 's>(state: &'s S, request: &'a Request) -> Result<Self, Self::Rejection>;
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
            // .get("free", |_r: Request| {})
            // .get("/bar2", bar2)
            .finish();

        let mut state = AppState;

        let request = Request;

        let response = app.call(&mut state, request);

        // .route("/foo", get(get_foo).post(post_foo))
        // .route("/foo/bar", get(foo_bar));
    }
}
