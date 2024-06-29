//! HTTP/1.1 client

mod call;
pub use call::Call;

mod state;
pub use state::State;

mod amended;

mod holder;

/// Type state for requests without bodies via [`Call::without_body()`]
#[doc(hidden)]
pub struct WithoutBody(());

/// Type state for streaming bodies via [`Call::with_streaming_body()`]
#[doc(hidden)]
pub struct WithBody(());

/// Type state for receiving the HTTP Response
#[doc(hidden)]
pub struct RecvResponse(());

/// Type state for receiving the response body
#[doc(hidden)]
pub struct RecvBody(());

/// Max number of headers in an HTTP response
pub const MAX_HEADERS: usize = 100;

#[cfg(test)]
mod test {
    use super::*;

    use std::str;

    use http::{Method, Request};

    use crate::Error;

    #[test]
    fn ensure_send_sync() {
        fn is_send_sync<T: Send + Sync>(_t: T) {}

        is_send_sync(Call::without_body(&Request::new(())).unwrap());

        is_send_sync(Call::with_body(&Request::post("/").body(()).unwrap()).unwrap());
    }

    #[test]
    fn create_empty() {
        let req = Request::builder().body(()).unwrap();
        let _call = Call::without_body(&req);
    }

    #[test]
    fn create_streaming() {
        let req = Request::builder().body(()).unwrap();
        let _call = Call::with_body(&req);
    }

    #[test]
    fn head() {
        let req = Request::head("http://foo.test/page").body(()).unwrap();
        let mut call = Call::without_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let n = call.write(&mut output).unwrap();
        let s = str::from_utf8(&output[..n]).unwrap();

        assert_eq!(s, "HEAD /page HTTP/1.1\r\nhost: foo.test\r\n");
    }

    #[test]
    fn head_with_body() {
        let req = Request::head("http://foo.test/page").body(()).unwrap();
        let err = Call::with_body(&req).unwrap_err();

        assert_eq!(err, Error::MethodForbidsBody(Method::HEAD));
    }

    #[test]
    fn post_simple() {
        let req = Request::post("http://f.test/page")
            .header("content-length", 5)
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (i1, n1) = call.write(b"hallo", &mut output).unwrap();
        let (i2, n2) = call.write(b"hallo", &mut output[n1..]).unwrap();
        assert_eq!(i1, 0);
        assert_eq!(i2, 5);
        assert_eq!(n1, 54);
        assert_eq!(n2, 5);
        let s = str::from_utf8(&output[..n1 + n2]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ncontent-length: 5\r\nhallo"
        );
    }

    #[test]
    fn post_small_output() {
        let req = Request::post("http://f.test/page")
            .header("content-length", 5)
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];

        let body = b"hallo";

        {
            let (i, n) = call.write(body, &mut output[..25]).unwrap();
            assert_eq!(i, 0);
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "POST /page HTTP/1.1\r\n");
            assert!(!call.request_finished());
        }

        {
            let (i, n) = call.write(body, &mut output[..20]).unwrap();
            assert_eq!(i, 0);
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "host: f.test\r\n");
            assert!(!call.request_finished());
        }

        {
            let (i, n) = call.write(body, &mut output[..19]).unwrap();
            assert_eq!(i, 0);
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "content-length: 5\r\n");
            assert!(!call.request_finished());
        }

        {
            let (i, n) = call.write(body, &mut output[..25]).unwrap();
            assert_eq!(n, 5);
            assert_eq!(i, 5);
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "hallo");
            assert!(call.request_finished());
        }
    }

    #[test]
    fn post_with_short_content_length() {
        let req = Request::post("http://f.test/page")
            .header("content-length", 2)
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let body = b"hallo";

        let mut output = vec![0; 1024];
        let r = call.write(body, &mut output);
        assert!(r.is_ok());

        let r = call.write(body, &mut output);

        assert_eq!(r.unwrap_err(), Error::BodyLargerThanContentLength);
    }

    #[test]
    fn post_with_short_body_input() {
        let req = Request::post("http://f.test/page")
            .header("content-length", 5)
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (i1, n1) = call.write(b"ha", &mut output).unwrap();
        let (i2, n2) = call.write(b"ha", &mut output[n1..]).unwrap();
        assert_eq!(i1, 0);
        assert_eq!(i2, 2);
        assert_eq!(n1, 54);
        assert_eq!(n2, 2);
        let s = str::from_utf8(&output[..n1 + n2]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ncontent-length: 5\r\nha"
        );

        assert!(!call.request_finished());

        let (i, n2) = call.write(b"llo", &mut output).unwrap();
        assert_eq!(i, 3);
        let s = str::from_utf8(&output[..n2]).unwrap();

        assert_eq!(s, "llo");

        assert!(call.request_finished());
    }

    #[test]
    fn post_with_chunked() {
        let req = Request::post("http://f.test/page")
            .header("transfer-encoding", "chunked")
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let body = b"hallo";

        let mut output = vec![0; 1024];
        let (i1, n1) = call.write(body, &mut output).unwrap();
        let (i2, n2) = call.write(body, &mut output[n1..]).unwrap();
        let (_, n3) = call.write(&[], &mut output[n1 + n2..]).unwrap();
        assert_eq!(i1, 0);
        assert_eq!(i2, 5);
        assert_eq!(n1, 63);
        assert_eq!(n2, 10);
        assert_eq!(n3, 5);
        let s = str::from_utf8(&output[..n1 + n2 + n3]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ntransfer-encoding: chunked\r\n5\r\nhallo\r\n0\r\n\r\n"
        );
    }

    #[test]
    fn post_without_body() {
        let req = Request::post("http://foo.test/page").body(()).unwrap();
        let err = Call::without_body(&req).unwrap_err();

        assert_eq!(err, Error::MethodRequiresBody(Method::POST));
    }

    #[test]
    fn post_streaming() {
        let req = Request::post("http://f.test/page").body(()).unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (i1, n1) = call.write(b"hallo", &mut output).unwrap();
        let (i2, n2) = call.write(b"hallo", &mut output[n1..]).unwrap();

        // Send end
        let (i3, n3) = call.write(&[], &mut output[n1 + n2..]).unwrap();

        assert_eq!(i1, 0);
        assert_eq!(i2, 5);
        assert_eq!(n1, 63);
        assert_eq!(n2, 10);
        assert_eq!(i3, 0);
        assert_eq!(n3, 5);

        let s = str::from_utf8(&output[..(n1 + n2 + n3)]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ntransfer-encoding: chunked\r\n5\r\nhallo\r\n0\r\n\r\n"
        );
    }

    #[test]
    fn post_streaming_with_size() {
        let req = Request::post("http://f.test/page")
            .header("content-length", "5")
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (i1, n1) = call.write(b"hallo", &mut output).unwrap();
        let (i2, n2) = call.write(b"hallo", &mut output[n1..]).unwrap();
        assert_eq!(i1, 0);
        assert_eq!(n1, 54);
        assert_eq!(i2, 5);
        assert_eq!(n2, 5);

        let s = str::from_utf8(&output[..(n1 + n2)]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ncontent-length: 5\r\nhallo"
        );
    }

    #[test]
    fn post_streaming_after_end() {
        let req = Request::post("http://f.test/page").body(()).unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (_, n1) = call.write(b"hallo", &mut output).unwrap();

        // Send end
        let (_, n2) = call.write(&[], &mut output[n1..]).unwrap();

        let err = call.write(b"after end", &mut output[(n1 + n2)..]);

        assert_eq!(err, Err(Error::BodyContentAfterFinish));
    }

    #[test]
    fn post_streaming_too_much() {
        let req = Request::post("http://f.test/page")
            .header("content-length", "5")
            .body(())
            .unwrap();
        let mut call = Call::with_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (_, n1) = call.write(b"hallo", &mut output).unwrap();
        let (_, n2) = call.write(b"hallo", &mut output[n1..]).unwrap();

        // this is more than content-length
        let err = call.write(b"fail", &mut output[n1 + n2..]).unwrap_err();

        assert_eq!(err, Error::BodyContentAfterFinish);
    }
}
