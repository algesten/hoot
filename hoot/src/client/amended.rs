use http::{HeaderName, HeaderValue, Method, Request, Uri, Version};

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
/// 1.  Cookie headers. Cookie jar functionality is out of scope for hoot, but the
///     `headers from such a jar should be possible to add.
/// 2.  `Host` header. Taken from the Request URI unless already set.
/// 3.  `Content-Type` header. The actual request body handling is out of scope for hoot,
///     but an implementation must be able to autodetect the content type for a given body
///     and provide that on the request.
/// 4.  `Content-Length` header. When sending non chunked transfer bodies (and not HTTP/1.0
///     which closes the connection).
/// 5.  `Transfer-Encoding: chunked` header when the content length for a body is unknown.
/// 6.  `Content-Encoding` header to indicate on-the-wire compression. The compression itself
///      is out of scope for hoot, but the user must be able to set it.
/// 7.  `User-Agent` header.
/// 8.  `Accept` header.
/// 9.  Changing the `Method` when following redirects.
/// 10. Changing the `Uri` when following redirect.
///
pub(crate) struct AmendedRequest<'a> {
    request: &'a Request<()>,
    uri: Option<Uri>,
    headers: Vec<(HeaderName, HeaderValue)>,
    method: Method,
}

impl<'a> AmendedRequest<'a> {
    pub fn new(request: &'a Request<()>) -> Self {
        let method = request.method().clone();

        AmendedRequest {
            request,
            uri: None,
            headers: Vec::with_capacity(50),
            method,
        }
    }

    pub fn inner(&self) -> &'a Request<()> {
        self.request
    }

    pub fn set_uri(&mut self, uri: Uri) {
        self.uri = Some(uri);
    }

    pub fn uri(&self) -> &Uri {
        if let Some(uri) = &self.uri {
            uri
        } else {
            self.request.uri()
        }
    }

    pub fn line(&self) -> (&Method, &str, Version) {
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
        self.headers
            .iter()
            .map(|v| (&v.0, &v.1))
            .chain(self.request.headers().iter())
    }

    pub fn headers_len(&self) -> usize {
        self.headers().count()
    }

    pub fn set_method(&mut self, method: Method) {
        self.method = method;
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn new_uri_from_location(&self, location: &str) -> Result<Uri, Error> {
        let base = self.uri();

        // Parse the Location: header to a uri. This handles relative
        // uri as well as absolute.
        let mut parts = location
            .parse::<Uri>()
            .map_err(|_| Error::BadLocationHeader)?
            .into_parts();

        if parts.scheme.is_none() {
            parts.scheme = Some(base.scheme().cloned().expect("base scheme"));
        }

        if parts.authority.is_none() {
            parts.authority = Some(base.authority().cloned().expect("base authority"));
        }

        let uri = Uri::from_parts(parts).expect("valid uri from parts");

        Ok(uri)
    }
}
