mod call;
pub use call::Call;

/// Type state for requests without bodies via [`Call::without_body()`]
#[repr(transparent)]
#[doc(hidden)]
pub struct SendEmpty(());

/// Trait for types used to send owned bodies via [`Call::with_static_body()`]
#[doc(hidden)]
pub trait SendBody: AsRef<[u8]> {}

impl<T: AsRef<[u8]>> SendBody for T {}

/// Type state for streaming bodies via [`Call::with_streaming_body()`]
#[doc(hidden)]
#[repr(transparent)]
pub struct SendStream(());

/// Type state for receiving the HTTP Response
#[repr(transparent)]
#[doc(hidden)]
pub struct RecvResponse(());

/// Type state for receiving the response body
#[doc(hidden)]
pub struct RecvBody(());

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

        is_send_sync(Call::with_static_body(&Request::post("/").body(b"ha").unwrap()).unwrap());

        is_send_sync(Call::with_streaming_body(&Request::post("/").body(()).unwrap()).unwrap());
    }

    #[test]
    fn various_bodies() {
        let _ = Call::with_static_body(&Request::new(&[]));
        let _ = Call::with_static_body(&Request::new(&vec![]));
        let _ = Call::with_static_body(&Request::new(&[0, 1, 2]));
        let _ = Call::with_static_body(&Request::new(&"foo"));
        let _ = Call::with_static_body(&Request::new(b"foo"));
        let _ = Call::with_static_body(&Request::new(&"foo".to_string()));
        let _ = Call::with_static_body(&Request::new(vec![]));
        let _ = Call::with_static_body(&Request::new([0, 1, 2]));
        let _ = Call::with_static_body(&Request::new("foo"));
        let _ = Call::with_static_body(&Request::new("foo".to_string()));
    }

    #[test]
    fn create_empty() {
        let req = Request::builder().body(()).unwrap();
        let _call = Call::without_body(&req);
    }

    #[test]
    fn create_body() {
        let s = "hello";
        let req = Request::builder().body(&s).unwrap();
        let _call = Call::with_static_body(&req);

        let v = vec![0_u8, 1, 2];
        let req = Request::builder().body(&v).unwrap();
        let _call = Call::with_static_body(&req);

        let v: &[u8] = &[1_u8, 2, 3];
        let req = Request::builder().body(&v).unwrap();
        let _call = Call::with_static_body(&req);
    }

    #[test]
    fn create_streaming() {
        let req = Request::builder().body(()).unwrap();
        let _call = Call::with_streaming_body(&req);
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
        let req = Request::head("http://foo.test/page").body(&[]).unwrap();
        let err = Call::with_static_body(&req).unwrap_err();

        assert_eq!(err, Error::MethodForbidsBody(Method::HEAD));
    }

    #[test]
    fn post() {
        let req = Request::post("http://f.test/page").body(b"hallo").unwrap();
        let mut call = Call::with_static_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let n = call.write(&mut output).unwrap();
        let s = str::from_utf8(&output[..n]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ncontent-length: 5\r\nhallo"
        );
    }

    #[test]
    fn post_small_output() {
        let req = Request::post("http://f.test/page").body(b"hallo").unwrap();
        let mut call = Call::with_static_body(&req).unwrap();

        let mut output = vec![0; 1024];

        {
            let n = call.write(&mut output[..25]).unwrap();
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "POST /page HTTP/1.1\r\n");
        }

        {
            let n = call.write(&mut output[..20]).unwrap();
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "host: f.test\r\n");
        }

        {
            let n = call.write(&mut output[..19]).unwrap();
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "content-length: 5\r\n");
        }

        {
            let n = call.write(&mut output[..25]).unwrap();
            let s = str::from_utf8(&output[..n]).unwrap();
            assert_eq!(s, "hallo");
        }
    }

    #[test]
    fn post_with_short_content_length() {
        let req = Request::post("http://f.test/page")
            .header("content-length", 2)
            .body(b"hallo")
            .unwrap();
        let mut call = Call::with_static_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let n = call.write(&mut output).unwrap();
        let s = str::from_utf8(&output[..n]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ncontent-length: 2\r\nha"
        );
    }

    #[test]
    fn post_with_chunked() {
        let req = Request::post("http://f.test/page")
            .header("transfer-encoding", "chunked")
            .body(b"hallo")
            .unwrap();
        let mut call = Call::with_static_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let n = call.write(&mut output).unwrap();
        let s = str::from_utf8(&output[..n]).unwrap();

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
        let mut call = Call::with_streaming_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (in1, out1) = call.write(b"hallo", &mut output).unwrap();
        assert_eq!(in1, 5);
        assert_eq!(out1, 73);

        // Send end
        let (in2, out2) = call.write(&[], &mut output[out1..]).unwrap();
        assert_eq!(in2, 0);
        assert_eq!(out2, 5);

        let s = str::from_utf8(&output[..(out1 + out2)]).unwrap();

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
        let mut call = Call::with_streaming_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (in1, out1) = call.write(b"hallo", &mut output).unwrap();
        assert_eq!(in1, 5);
        assert_eq!(out1, 59);

        let s = str::from_utf8(&output[..(out1)]).unwrap();

        assert_eq!(
            s,
            "POST /page HTTP/1.1\r\nhost: f.test\r\ncontent-length: 5\r\nhallo"
        );
    }

    #[test]
    fn post_streaming_after_end() {
        let req = Request::post("http://f.test/page").body(()).unwrap();
        let mut call = Call::with_streaming_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (_, out1) = call.write(b"hallo", &mut output).unwrap();

        // Send end
        let (_, out2) = call.write(&[], &mut output[out1..]).unwrap();

        let err = call.write(b"after end", &mut output[(out1 + out2)..]);

        assert_eq!(err, Err(Error::StreamingContentAfterFinish));
    }

    #[test]
    fn post_streaming_too_much() {
        let req = Request::post("http://f.test/page")
            .header("content-length", "5")
            .body(())
            .unwrap();
        let mut call = Call::with_streaming_body(&req).unwrap();

        let mut output = vec![0; 1024];
        let (_, n) = call.write(b"hallo", &mut output).unwrap();

        // this is more than content-length
        let err = call.write(b"fail", &mut output[n..]).unwrap_err();

        assert_eq!(err, Error::BodyLargerThanContentLength);
    }
}
