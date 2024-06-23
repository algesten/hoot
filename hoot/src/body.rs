use std::fmt;
use std::io::Write;

use http::Method;

use crate::chunk::Dechunker;
use crate::util::{compare_lowercase_ascii, Writer};
use crate::Error;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum BodyMode {
    #[default]
    None,
    Sized(u64),
    Chunked,
}

const DEFAULT_CHUNK_SIZE: usize = 10 * 1024;

impl BodyMode {
    pub fn has_body(&self) -> bool {
        matches!(self, BodyMode::Sized(_) | BodyMode::Chunked)
    }

    pub fn is_chunked(&self) -> bool {
        matches!(self, Self::Chunked)
    }

    pub fn write(&mut self, input: &[u8], w: &mut Writer) -> usize {
        match self {
            BodyMode::None => unreachable!(),
            BodyMode::Sized(left) => {
                let left_usize = (*left).min(usize::MAX as u64) as usize;
                let to_write = w.available().min(input.len()).min(left_usize);

                let success = w.try_write(|w| w.write_all(&input[..to_write]));
                assert!(success);

                *left -= to_write as u64;

                to_write
            }
            BodyMode::Chunked => {
                let mut input_used = 0;

                // The chunk size might be smaller than the entire input, in which case
                // we continue to send chunks frome the same input.
                while write_chunk(&input[input_used..], &mut input_used, w, DEFAULT_CHUNK_SIZE) {}

                input_used
            }
        }
    }

    pub(crate) fn finish(&self, w: &mut Writer) -> bool {
        if self.is_chunked() {
            let success = w.try_write(|w| w.write_all(b"0\r\n\r\n"));
            if !success {
                return false;
            }
        }
        true
    }
}

fn write_chunk(input: &[u8], input_used: &mut usize, w: &mut Writer, max_chunk: usize) -> bool {
    // 5 is the smallest possible overhead
    let available = w.available().saturating_sub(5);

    let to_write = input.len().min(max_chunk).min(available);

    // we don't want to write 0 since that indicates end-of-body.
    if to_write == 0 {
        return false;
    }

    let success = w.try_write(|w| {
        // chunk length
        write!(w, "{:0x?}\r\n", to_write)?;

        // chunk
        w.write_all(&input[..to_write])?;

        // chunk end
        write!(w, "\r\n")
    });

    if success {
        *input_used += to_write;
    }

    // write another chunk?
    success && input.len() > to_write
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BodyReader {
    /// No body is expected either due to the status or method.
    NoBody,
    /// Delimited by content-length.
    /// The value is what's left to receive.
    LengthDelimited(u64),
    /// Chunked transfer encoding
    Chunked(Dechunker),
    /// Expect remote to close at end of body.
    CloseDelimited,
}

impl BodyReader {
    // pub fn for_request<'a>(
    //     http10: bool,
    //     method: &Method,
    //     header_lookup: &'a dyn Fn(&str) -> Option<&'a str>,
    // ) -> Result<Self, Error> {
    //     let has_no_body = !method.need_request_body();

    //     if has_no_body {
    //         return Ok(Self::LengthDelimited(0));
    //     }

    //     let ret = match Self::header_defined(http10, header_lookup)? {
    //         // Request bodies cannot be close delimited (even under http10).
    //         Self::CloseDelimited => Self::LengthDelimited(0),
    //         r @ _ => r,
    //     };

    //     Ok(ret)
    // }

    pub fn for_response<'a>(
        http10: bool,
        method: &Method,
        status_code: u16,
        header_lookup: &'a dyn Fn(&str) -> Option<&'a str>,
    ) -> Result<Self, Error> {
        let is_success = (200..=299).contains(&status_code);
        let is_informational = (100..=199).contains(&status_code);

        let has_no_body =
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
            // All responses to the HEAD request method
            // MUST NOT include a message-body, even though the presence of entity-
            // header fields might lead one to believe they do.
            method == Method::HEAD ||
            // A client MUST ignore any Content-Length or Transfer-Encoding
            // header fields received in a successful response to CONNECT.
            is_success && method == Method::CONNECT ||
            // All 1xx (informational), 204 (no content), and 304 (not modified) responses
            // MUST NOT include a message-body.
            is_informational ||
            matches!(status_code, 204 | 304);

        if has_no_body {
            return Ok(Self::NoBody);
        }

        // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
        // All other responses do include a message-body, although it MAY be of zero length.
        Self::header_defined(http10, header_lookup)
    }

    fn header_defined<'a>(
        http10: bool,
        header_lookup: &'a dyn Fn(&str) -> Option<&'a str>,
    ) -> Result<Self, Error> {
        let mut content_length: Option<u64> = None;
        let mut chunked = false;

        // for head in headers {
        if let Some(value) = header_lookup("content-length") {
            let v = value
                .parse::<u64>()
                .map_err(|_| Error::BadContentLengthHeader)?;
            if content_length.is_some() {
                return Err(Error::TooManyContentLengthHeaders);
            }
            content_length = Some(v);
        }

        if let Some(value) = header_lookup("transfer-encoding") {
            // Header can repeat, stop looking if we found "chunked"
            chunked = value
                .split(',')
                .map(|v| v.trim())
                .any(|v| compare_lowercase_ascii(v, "chunked"));
        }

        if chunked && !http10 {
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
            // Messages MUST NOT include both a Content-Length header field and a
            // non-identity transfer-coding. If the message does include a non-
            // identity transfer-coding, the Content-Length MUST be ignored.
            return Ok(Self::Chunked(Dechunker::new()));
        }

        if let Some(len) = content_length {
            return Ok(Self::LengthDelimited(len));
        }

        Ok(Self::CloseDelimited)
    }

    pub fn read(&mut self, src: &[u8], dst: &mut [u8]) -> Result<(usize, usize), Error> {
        trace!("Read body");

        // unwrap is ok because we can't be in state RECV_BODY without setting it.
        let part = match self {
            BodyReader::LengthDelimited(_) => self.read_limit(src, dst),
            BodyReader::Chunked(_) => self.read_chunked(src, dst),
            BodyReader::CloseDelimited => self.read_unlimit(src, dst),
            BodyReader::NoBody => return Ok((0, 0)),
        }?;

        Ok(part)
    }

    fn read_limit(&mut self, src: &[u8], dst: &mut [u8]) -> Result<(usize, usize), Error> {
        let Self::LengthDelimited(left) = self else {
            unreachable!()
        };
        let left_usize = (*left).min(usize::MAX as u64) as usize;

        let to_read = src.len().min(dst.len()).min(left_usize);

        dst[..to_read].copy_from_slice(&src[..to_read]);

        *left -= to_read as u64;

        Ok((to_read, to_read))
    }

    fn read_chunked(&mut self, src: &[u8], dst: &mut [u8]) -> Result<(usize, usize), Error> {
        let BodyReader::Chunked(dechunker) = self else {
            unreachable!();
        };

        let (input_used, output_used) = dechunker.parse_input(src, dst)?;

        trace!("Read chunked: {}", input_used);

        Ok((input_used, output_used))
    }

    fn read_unlimit(&mut self, src: &[u8], dst: &mut [u8]) -> Result<(usize, usize), Error> {
        let to_read = src.len().min(dst.len());

        dst[..to_read].copy_from_slice(&src[..to_read]);

        Ok((to_read, to_read))
    }

    pub fn is_ended(&self) -> bool {
        match self {
            BodyReader::NoBody => true,
            BodyReader::LengthDelimited(v) => *v == 0,
            BodyReader::Chunked(v) => v.is_ended(),
            BodyReader::CloseDelimited => false,
        }
    }
}

impl fmt::Debug for BodyReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBody => write!(f, "NoBody"),
            Self::LengthDelimited(arg0) => f.debug_tuple("LengthDelimited").field(arg0).finish(),
            Self::Chunked(_) => write!(f, "Chunked"),
            Self::CloseDelimited => write!(f, "CloseDelimited"),
        }
    }
}
