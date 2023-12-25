mod req;
pub use req::{Line, Request};

mod res;
pub use res::{Response, ResponseVariant, ResumeToken};

#[cfg(test)]
mod test {
    use crate::error::Result;
    use crate::HttpVersion;
    use crate::Method;

    use super::*;

    #[test]
    pub fn test_res_get() -> Result<()> {
        let mut buf = [0; 1024];

        // ************* READ REQUEST LINE *****************

        let mut request = Request::new();

        // Try read incomplete input. The provided buffer is required to parse request headers.
        let attempt = request.try_read_request(b"GET /path HTTP/1.", &mut buf)?;
        assert!(!attempt.is_success());

        const COMPLETE: &[u8] = b"GET /path HTTP/1.1\r\nHost: foo\r\nContent-Length: 10\r\n\r\n";

        // Try read complete input (and succeed). Borrow the buffer again.
        let attempt = request.try_read_request(COMPLETE, &mut buf)?;
        assert!(attempt.is_success());

        // Read request line information.
        let line = attempt.line().unwrap();
        assert_eq!(line.version(), HttpVersion::Http11);
        assert_eq!(line.method(), Method::GET);
        assert_eq!(line.path(), "/path");

        // Read headers.
        let headers = attempt.headers().unwrap();
        assert_eq!(headers[0].name(), "Host");
        assert_eq!(headers[0].value(), "foo");
        assert_eq!(headers[1].name(), "Content-Length");
        assert_eq!(headers[1].value(), "10");

        // Proceed to reading request body.
        let request = request.proceed();

        // GET requests have no body.
        assert!(request.is_finished());

        // ************* SERVE RESPONSE *****************

        // This gives us variants, i.e. methods, that we can respond differently to.
        let variants = request.into_response()?;

        // This matches out a response token with the type state that helps us form
        // a correct response.
        let token = match variants {
            ResponseVariant::Get(v) => v,
            ResponseVariant::Head(_) => todo!(),
            ResponseVariant::Post(_) => todo!(),
            ResponseVariant::Put(_) => todo!(),
            ResponseVariant::Delete(_) => todo!(),
            ResponseVariant::Connect(_) => todo!(),
            ResponseVariant::Options(_) => todo!(),
            ResponseVariant::Trace(_) => todo!(),
            ResponseVariant::Patch(_) => todo!(),
        };

        let response = Response::resume(token, &mut buf);

        response
            .send_status(200, "Ok")?
            .header("content-type", "application/json")?;

        Ok(())
    }
}
