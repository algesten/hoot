use alloc::string::ToString;
use core::mem;

use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Uri, Version};
use url::Url;

use crate::body::BodyWriter;
use crate::ext::MethodExt;
use crate::util::{compare_lowercase_ascii, ArrayVec};
use crate::Error;

use super::MAX_EXTRA_HEADERS;

/// `Request` with amends.
///
/// The user provides the `Request<()>`, which we consider an immutable object.
/// When executing a request there are a couple of changes/overrides required to
/// that immutable object. The `AmendedRequest` encapsulates the original request
/// and the amends.
///
/// The expected amends are:
///
/// 1.  Cookie headers. Cookie jar functionality is out of scope, but the
///     `headers from such a jar should be possible to add.
/// 2.  `Host` header. Taken from the Request URI unless already set.
/// 3.  `Content-Type` header. The actual request body handling is out of scope,
///     but an implementation must be able to autodetect the content type for a given body
///     and provide that on the request.
/// 4.  `Content-Length` header. When sending non chunked transfer bodies (and not HTTP/1.0
///     which closes the connection).
/// 5.  `Transfer-Encoding: chunked` header when the content length for a body is unknown.
/// 6.  `Content-Encoding` header to indicate on-the-wire compression. The compression itself
///      is out of scope, but the user must be able to set it.
/// 7.  `User-Agent` header.
/// 8.  `Accept` header.
/// 9.  Changing the `Method` when following redirects.
/// 10. Changing the `Uri` when following redirect.
///
pub(crate) struct AmendedRequest<Body> {
    request: Request<Option<Body>>,
    uri: Option<Uri>,
    headers: ArrayVec<(HeaderName, HeaderValue), MAX_EXTRA_HEADERS>,
    unset: ArrayVec<HeaderName, 3>,
}

impl<Body> AmendedRequest<Body> {
    pub fn new(request: Request<Body>) -> Self {
        let (parts, body) = request.into_parts();

        const UNINIT_NAME: HeaderName = HeaderName::from_static("x-null");
        const UNINIT_VALUE: HeaderValue = HeaderValue::from_static("");

        AmendedRequest {
            request: Request::from_parts(parts, Some(body)),
            uri: None,
            headers: ArrayVec::from_fn(|_| (UNINIT_NAME, UNINIT_VALUE)),
            unset: ArrayVec::from_fn(|_| UNINIT_NAME),
        }
    }

    pub fn take_request(&mut self) -> Request<Body> {
        let request = mem::replace(&mut self.request, Request::new(None));
        let (parts, body) = request.into_parts();
        Request::from_parts(parts, body.unwrap())
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
            self.uri()
                .path_and_query()
                .map(|p| p.as_str())
                .unwrap_or("/"),
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

    pub fn unset_header<K>(&mut self, name: K) -> Result<(), Error>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
    {
        let name = <HeaderName as TryFrom<K>>::try_from(name)
            .map_err(Into::into)
            .map_err(|e| Error::BadHeader(e.to_string()))?;
        self.unset.push(name);
        Ok(())
    }

    pub fn original_request_headers(&self) -> &HeaderMap {
        self.request.headers()
    }

    pub fn headers(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)> {
        self.headers
            .iter()
            .map(|v| (&v.0, &v.1))
            .chain(self.request.headers().iter())
            .filter(|v| !self.unset.iter().any(|x| x == v.0))
    }

    fn headers_get_all(&self, key: &'static str) -> impl Iterator<Item = &HeaderValue> {
        self.headers()
            .filter(move |(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    fn headers_get(&self, key: &'static str) -> Option<&HeaderValue> {
        self.headers_get_all(key).next()
    }

    pub fn headers_len(&self) -> usize {
        self.headers().count()
    }

    #[cfg(test)]
    pub fn headers_vec(&self) -> alloc::vec::Vec<(&str, &str)> {
        self.headers()
            // unwrap here is ok because the tests using this method should
            // only use header values representable as utf-8.
            // If we want to test non-utf8 header values, use .headers()
            // iterator instead.
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect()
    }

    pub fn method(&self) -> &Method {
        self.request.method()
    }

    pub(crate) fn version(&self) -> Version {
        self.request.version()
    }

    pub fn new_uri_from_location(&self, location: &str) -> Result<Uri, Error> {
        let base = Url::parse(&self.uri().to_string()).expect("base uri to be a url");

        let url = base
            .join(location)
            .map_err(|_| Error::BadLocationHeader(location.to_string()))?;

        let uri = url
            .to_string()
            .parse::<Uri>()
            .map_err(|_| Error::BadLocationHeader(url.to_string()))?;

        Ok(uri)
    }

    pub fn analyze(
        &self,
        wanted_mode: BodyWriter,
        skip_method_body_check: bool,
    ) -> Result<RequestInfo, Error> {
        let v = self.request.version();
        let m = self.method();

        m.verify_version(v)?;

        let count_host = self.headers_get_all("host").count();
        if count_host > 1 {
            return Err(Error::TooManyHostHeaders);
        }

        let count_len = self.headers_get_all("content-length").count();
        if count_len > 1 {
            return Err(Error::TooManyContentLengthHeaders);
        }

        let mut req_host_header = false;
        if let Some(h) = self.headers_get("host") {
            h.to_str().map_err(|_| Error::BadHostHeader)?;
            req_host_header = true;
        }

        let mut content_length: Option<u64> = None;
        if let Some(h) = self.headers_get("content-length") {
            let n = h
                .to_str()
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or(Error::BadContentLengthHeader)?;
            content_length = Some(n);
        }

        let has_chunked = self
            .headers_get_all("transfer-encoding")
            .filter_map(|v| v.to_str().ok())
            .any(|v| compare_lowercase_ascii(v, "chunked"));

        let mut req_body_header = false;

        // https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
        // Messages MUST NOT include both a Content-Length header field and a
        // non-identity transfer-coding. If the message does include a non-
        // identity transfer-coding, the Content-Length MUST be ignored.
        let body_mode = if has_chunked {
            // chunked "wins"
            req_body_header = true;
            BodyWriter::new_chunked()
        } else if let Some(n) = content_length {
            // user provided content-length second
            req_body_header = true;
            BodyWriter::new_sized(n)
        } else {
            wanted_mode
        };

        if !skip_method_body_check {
            let need_body = self.method().need_request_body();
            let has_body = body_mode.has_body();

            if !need_body && has_body {
                return Err(Error::MethodForbidsBody(self.method().clone()));
            } else if need_body && !has_body {
                return Err(Error::MethodRequiresBody(self.method().clone()));
            }
        }

        Ok(RequestInfo {
            body_mode,
            req_host_header,
            req_body_header,
        })
    }
}

pub(crate) struct RequestInfo {
    pub body_mode: BodyWriter,
    pub req_host_header: bool,
    pub req_body_header: bool,
}
