use http::{HeaderName, HeaderValue, Method, Request, Uri, Version};
use once_cell::sync::OnceCell;

use crate::analyze::HeaderIterExt;
use crate::Error;

/// `Request` with amends.
///
/// The user provides the `Request<()>`, which hoot considers an immutable object.
/// When executing a request there are a couple of changes/overrides required to
/// that immutable object. The `AmendedRequest` encapsulates the original request
/// and the amends.
///
/// The expected amends are:
///
/// 1. Cookie headers. Cookie jar functionality is out of scope for hoot, but the
///    headers from such a jar should be possible to add.
/// 2. `Host` header. Taken from the Request URI unless already set.
/// 3. `Content-Type` header. The actual request body handling is out of scope for hoot,
///    but an implementation must be able to autodetect the content type for a given body
///    and provide that on the request.
/// 4. `Content-Length` header. When sending non chunked transfer bodies (and not HTTP/1.0
///    which closes the connection).
/// 5. `Transfer-Encoding: chunked` header when the content length for a body is unknown.
/// 6. `Content-Encoding` header to indicate on-the-wire compression. The compression itself
///     is out of scope for hoot, but the user must be able to set it.
/// 7. `User-Agent` header.
/// 8. `Accept` header.
/// 9. Changing the `Method` when following redirects.
///
/// The request can be in a "released" state, where the original request is not retained.
/// This is used when the call goes into `RecvBody`, since there is not reason to hang on
/// to the request object beyond that point. A possible use case would be to dispatch request
/// and use threads to read the response and response bodies.
pub(crate) struct AmendedRequest<'a> {
    request: &'a Request<()>,
    headers: Vec<(HeaderName, HeaderValue)>,
    method: Method,
    released: bool,
}

impl<'a> AmendedRequest<'a> {
    pub fn new(request: &'a Request<()>) -> Self {
        let method = request.method().clone();

        AmendedRequest {
            request,
            headers: Vec::with_capacity(50),
            method,
            released: false,
        }
    }

    pub fn uri(&self) -> &Uri {
        assert!(!self.released, "Get URI on released request");

        self.request.uri()
    }

    pub fn line(&self) -> (&Method, &str, Version) {
        assert!(!self.released, "Get request line on released request");

        let r = &self.request;
        (
            r.method(),
            r.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/"),
            r.version(),
        )
    }

    pub fn set_header<K, V>(&mut self, name: K, value: V) -> Result<(), Error>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        assert!(!self.released, "Set header on released request");

        let name = <HeaderName as TryFrom<K>>::try_from(name)
            .map_err(Into::into)
            .map_err(|e| Error::BadHeader(e.to_string()))?;
        let value = <HeaderValue as TryFrom<V>>::try_from(value)
            .map_err(Into::into)
            .map_err(|e| Error::BadHeader(e.to_string()))?;
        self.headers.push((name, value));
        Ok(())
    }

    pub fn headers(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)> {
        assert!(!self.released, "Get headers on released request");

        self.headers
            .iter()
            .map(|v| (&v.0, &v.1))
            .chain(self.request.headers().iter())
    }

    pub fn headers_len(&self) -> usize {
        assert!(!self.released, "Get headers_len on released request");

        self.headers().count()
    }

    // pub fn get_header(&self, key: &str) -> Option<&HeaderValue> {
    //     // First search local headers
    //     self.headers
    //         .iter()
    //         .find(|(k, _)| k == key)
    //         .as_ref()
    //         .map(|v| &v.1)
    //         // Fall back on request headers
    //         .or_else(|| self.request.headers().get(key))
    // }

    pub fn set_method(&mut self, method: Method) {
        assert!(!self.released, "Set method on released request");

        self.method = method;
    }

    pub fn method(&self) -> &Method {
        // This is allowed also on a released request.
        &self.method
    }

    pub fn into_released<'b>(self) -> AmendedRequest<'b> {
        assert!(!self.released, "Release a released request");

        // TODO(martin): is there a way to avoid a lock here? That would let us avoid
        // the once_cell dependency (which can be dropped if MSRV is 1.70).
        static EMPTY_REQUEST: OnceCell<Request<()>> = OnceCell::new();

        // unwrap is ok because building a request like this should not fail.
        let request = EMPTY_REQUEST.get_or_init(|| Request::builder().body(()).unwrap());

        AmendedRequest {
            request,
            headers: Vec::with_capacity(0),
            method: self.method,
            released: true,
        }
    }

    pub fn is_expect_100(&self) -> bool {
        self.headers().has("expect", "100-continue")
    }
}
