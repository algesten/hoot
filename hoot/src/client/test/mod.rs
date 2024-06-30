mod scenario;

mod test_prepare;

mod test_send_request;

mod test_send_body;

mod test_recv_response;

trait TestSliceExt {
    fn as_str(&self) -> &str;
}

impl TestSliceExt for [u8] {
    fn as_str(&self) -> &str {
        std::str::from_utf8(self).unwrap()
    }
}
