use http::{HeaderName, HeaderValue, Method, Request, Uri, Version};
use smallvec::SmallVec;
use url::Url;

use crate::Error;

use super::MAX_EXTRA_HEADERS;

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
    headers: SmallVec<[(HeaderName, HeaderValue); MAX_EXTRA_HEADERS]>,
    method: Method,
}

impl<'a> AmendedRequest<'a> {
    pub fn new(request: &'a Request<()>) -> Self {
        let method = request.method().clone();

        AmendedRequest {
            request,
            uri: None,
            headers: SmallVec::new(),
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

    pub fn prelude(&self) -> (&Method, &str, Version) {
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

    #[cfg(test)]
    pub fn headers_vec(&self) -> Vec<(&str, &str)> {
        self.headers()
            // unwrap here is ok because the tests using this method should
            // only use header values representable as utf-8.
            // If we want to test non-utf8 header values, use .headers()
            // iterator instead.
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect()
    }

    pub fn set_method(&mut self, method: Method) {
        self.method = method;
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn new_uri_from_location(&self, location: &str) -> Result<Uri, Error> {
        let base = Url::parse(&self.uri().to_string()).expect("base uri to be a url");

        let url = base.join(location).map_err(|_| Error::BadLocationHeader)?;

        let uri = url.to_string().parse::<Uri>().expect("new uri to parse");

        Ok(uri)
    }
}
