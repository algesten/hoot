use std::io;

pub trait Acceptor {
    type Reader: io::Read + Send + 'static;
    type Writer: io::Write + Send + 'static;
    type Breaker: Breaker + Send + 'static;

    fn accept(&mut self) -> io::Result<(Self::Reader, Self::Writer, Self::Breaker)>;
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
    use std::io::{self, Cursor};
    use std::sync::mpsc;

    use super::{Acceptor, Breaker};

    pub struct TestAcceptor(Option<Vec<u8>>);

    impl TestAcceptor {
        pub fn new(v: impl Into<Vec<u8>>) -> Self {
            TestAcceptor(Some(v.into()))
        }
    }

    impl Acceptor for TestAcceptor {
        type Reader = Cursor<Vec<u8>>;
        type Writer = TestWriter;
        type Breaker = TestBreaker;

        fn accept(&mut self) -> std::io::Result<(Self::Reader, Self::Writer, Self::Breaker)> {
            todo!()
        }
    }

    pub struct TestWriter(mpsc::SyncSender<Vec<u8>>);

    impl io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let len = buf.len();
            self.0
                .send(buf.to_vec())
                .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "test send failed"))?;
            Ok(len)
        }

        fn flush(&mut self) -> io::Result<()> {
            todo!()
        }
    }

    pub struct TestBreaker;

    impl Breaker for TestBreaker {
        fn disconnect(self) -> io::Result<()> {
            // TODO(martin): this should disconnect
            Ok(())
        }
    }
}
