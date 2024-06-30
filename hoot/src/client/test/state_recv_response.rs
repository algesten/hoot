use http::{StatusCode, Version};

use crate::client::test::scenario::Scenario;

// This is a complete response.
const RESPONSE: &[u8] = b"\
        HTTP/1.1 200 OK\r\n\
        Content-Length: 123\r\n\
        Content-Type: text/plain\r\n\
        \r\n";

#[test]
fn receive_incomplete_response() {
    // -1 to never reach the end
    for i in 0..RESPONSE.len() - 1 {
        let scenario = Scenario::builder().get("https://q.test").build();
        let mut flow = scenario.to_recv_response();

        let (input_used, maybe_response) = flow.try_response(&RESPONSE[..i]).unwrap();
        assert_eq!(input_used, 0);
        assert!(maybe_response.is_none());
        assert!(!flow.can_proceed());
    }
}

#[test]
fn receive_complete_response() {
    let scenario = Scenario::builder().get("https://q.test").build();
    let mut flow = scenario.to_recv_response();

    let (input_used, maybe_response) = flow.try_response(&RESPONSE).unwrap();
    assert_eq!(input_used, 66);
    assert!(maybe_response.is_some());

    let response = maybe_response.unwrap();

    assert_eq!(response.version(), Version::HTTP_11);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-length").unwrap(), "123");
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/plain"
    );

    assert!(flow.can_proceed());
}
