use std::io;

pub trait Acceptor {
    type Reader: io::Read + Send + 'static;
    type Writer: io::Write + Send + 'static;
    type Breaker: Breaker + Send + 'static;

    fn accept(&mut self) -> io::Result<(Self::Reader, Self::Writer, Self::Breaker)>;
}

impl Breaker for () {
    fn disconnect(self) -> io::Result<()> {
        Ok(())
    }
}

pub trait Breaker {
    fn disconnect(self) -> io::Result<()>;
}

pub mod tcp {
    use std::io;
    use std::net::{Shutdown, TcpListener, TcpStream};

    use super::{Acceptor, Breaker};

    pub struct TcpAcceptor(pub TcpListener);

    impl Acceptor for TcpAcceptor {
        type Reader = TcpStream;
        type Writer = TcpStream;
        type Breaker = TcpStreamBreaker;

        fn accept(&mut self) -> io::Result<(Self::Reader, Self::Writer, Self::Breaker)> {
            let (stream1, _) = self.0.accept()?;
            let stream2 = stream1.try_clone()?;
            let stream3 = stream1.try_clone()?;
            Ok((stream1, stream2, TcpStreamBreaker(stream3)))
        }
    }

    pub struct TcpStreamBreaker(TcpStream);

    impl Breaker for TcpStreamBreaker {
        fn disconnect(self) -> io::Result<()> {
            self.0.shutdown(Shutdown::Both)
        }
    }
}

pub mod test {
    use std::io::{self, BufRead, Read, Write};

    use hoot::types::state::{ENDED, RECV_RESPONSE, SEND_HEADERS};
    use hoot::types::version::HTTP_11;
    use hoot::BodyWriter;
    use http::{HeaderName, HeaderValue, Method};

    use crate::body::HootBody;
    use crate::fill_more::FillMoreBuffer;
    use crate::{http, Body, Error, Request};

    use super::Acceptor;

    pub struct TestAcceptor {
        request: Option<http::Request<Body>>,
    }

    impl TestAcceptor {
        pub fn new<B: Into<Body>>(req: http::Request<B>) -> Self {
            let (parts, body) = req.into_parts();
            Self {
                request: Some(http::Request::from_parts(parts, body.into())),
            }
        }
    }

    impl Acceptor for TestAcceptor {
        type Reader = TestReader;
        type Writer = TestWriter;
        type Breaker = ();

        fn accept(&mut self) -> std::io::Result<(Self::Reader, Self::Writer, Self::Breaker)> {
            let mut req = self.request.take().expect("TestAcceptor has one request");
            let mut write = io::Cursor::new(vec![]);

            let hoot_res =
                write_request(&mut req, &mut write).expect("no error writing test request");

            let r = {
                write.set_position(0);
                TestReader { request: write }
            };

            let w = TestWriter {
                response: io::Cursor::new(vec![]),
                hoot_res,
            };

            Ok((r, w, ()))
        }
    }

    fn write_request(
        req: &mut Request,
        write: &mut impl Write,
    ) -> Result<hoot::client::Response<RECV_RESPONSE>, Error> {
        let mut buf = vec![0; 10 * 1024];

        let hoot_req = hoot::client::Request::new(&mut buf).http_11();

        let host = req.uri().host().expect("test request URI to have a host");
        let path = req.uri().path();
        let m = req.method();
        let hs = req.headers().iter();

        let output = if m == Method::GET {
            write_headers(hs, hoot_req.get(host, path)?, write)?
                .send()?
                .write_to(write)?
                .flush()
        } else if m == Method::OPTIONS {
            write_headers(hs, hoot_req.options(host, path)?, write)?
                .send()?
                .write_to(write)?
                .flush()
        } else if m == Method::POST {
            let hoot_req = write_headers(hs, hoot_req.post(host, path)?, write)?;
            write_body(req.body_mut(), hoot_req, write)?
        } else if m == Method::PUT {
            let hoot_req = write_headers(hs, hoot_req.put(host, path)?, write)?;
            write_body(req.body_mut(), hoot_req, write)?
        } else if m == Method::DELETE {
            write_headers(hs, hoot_req.delete(host, path)?, write)?
                .send()?
                .write_to(write)?
                .flush()
        } else if m == Method::HEAD {
            write_headers(hs, hoot_req.head(host, path)?, write)?
                .send()?
                .write_to(write)?
                .flush()
        } else if m == Method::TRACE {
            write_headers(hs, hoot_req.trace(host, path)?, write)?
                .send()?
                .write_to(write)?
                .flush()
        } else if m == Method::CONNECT {
            write_headers(hs, hoot_req.connect(host, path)?, write)?
                .send()?
                .write_to(write)?
                .flush()
        } else if m == Method::PATCH {
            let hoot_req = write_headers(hs, hoot_req.patch(host, path)?, write)?;
            write_body(req.body_mut(), hoot_req, write)?
        } else {
            unreachable!("unimplemented HTTP method")
        };

        fn write_headers<'a, 'b, M: hoot::types::Method>(
            mut hs: impl Iterator<Item = (&'a HeaderName, &'a HeaderValue)>,
            mut hoot_req: hoot::client::Request<'b, SEND_HEADERS, HTTP_11, M, ()>,
            write: &mut impl Write,
        ) -> Result<hoot::client::Request<'b, SEND_HEADERS, HTTP_11, M, ()>, Error> {
            // flush out status line
            hoot_req = hoot_req.write_to(write)?;

            while let Some((name, value)) = hs.next() {
                hoot_req = hoot_req
                    .header_bytes(name.as_str(), value.as_bytes())?
                    .write_to(write)?;
            }

            Ok(hoot_req)
        }

        fn write_body<'a, 'b, M: hoot::types::MethodWithRequestBody>(
            body: &'a mut Body,
            hoot_req: hoot::client::Request<'b, SEND_HEADERS, HTTP_11, M, ()>,
            write: &mut impl Write,
        ) -> Result<hoot::client::Output<'b, ENDED, (), (), ()>, Error> {
            // TODO can we use the buffer in hoot directly?
            // The -30 is because if we do chunked transfer every chunk needs
            // a bit of overhead.
            let mut tmp = vec![0; 10 * 1024 - 30];

            if let Some(size) = body.size() {
                let mut hoot_req = hoot_req.with_body(size)?.write_to(write)?;

                loop {
                    let n = body.read(&mut tmp)?;
                    if n == 0 {
                        break;
                    }
                    hoot_req = hoot_req.write_bytes(&tmp[..n])?.write_to(write)?;
                }

                Ok(hoot_req.finish()?.write_to(write)?.flush())
            } else {
                let mut hoot_req = hoot_req.with_chunked()?.write_to(write)?;

                loop {
                    let n = body.read(&mut tmp)?;
                    if n == 0 {
                        break;
                    }
                    hoot_req = hoot_req.write_bytes(&tmp[..n])?.write_to(write)?;
                }

                Ok(hoot_req.finish()?.write_to(write)?.flush())
            }
        }

        Ok(output.into_response())
    }

    pub struct TestReader {
        request: io::Cursor<Vec<u8>>,
    }

    impl io::Read for TestReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.request.read(buf)
        }
    }

    pub struct TestWriter {
        response: io::Cursor<Vec<u8>>,
        hoot_res: hoot::client::Response<RECV_RESPONSE>,
    }

    impl TryFrom<TestWriter> for http::Response<Body> {
        type Error = Error;

        fn try_from(value: TestWriter) -> Result<Self, Self::Error> {
            let TestWriter {
                mut response,
                mut hoot_res,
            } = value;

            response.set_position(0);
            let mut tmp = vec![0; 10 * 1024];

            let attempt = hoot_res.try_read_response(response.remaining(), &mut tmp)?;
            assert!(attempt.is_success());

            let amt = attempt.input_used();

            let res: http::Response<()> = attempt.try_into()?;
            let (parts, _) = res.into_parts();

            response.consume(amt);
            let response: Box<dyn io::Read + 'static> = Box::new(response);

            let hoot_res = hoot_res.proceed();

            let buffer = FillMoreBuffer::new(response);

            let hoot_body = HootBody::new(hoot_res, tmp, buffer);
            let body = Body::hoot(hoot_body);

            Ok(http::Response::from_parts(parts, body))
        }
    }

    impl io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.response.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.response.flush()
        }
    }

    trait CursorRemaining {
        fn remaining(&self) -> &[u8];
    }

    impl CursorRemaining for std::io::Cursor<Vec<u8>> {
        fn remaining(&self) -> &[u8] {
            let p = self.position() as usize;
            &self.get_ref()[p..]
        }
    }
}
