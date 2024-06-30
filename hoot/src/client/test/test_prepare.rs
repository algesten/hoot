use super::scenario::Scenario;

#[test]
fn proceed_without_amended_headers() {
    let scenario = Scenario::builder().get("https://q.test").build();

    let state = scenario.to_prepare();

    let inner = state.inner();
    let request = inner.call.request();

    assert_eq!(request.headers_vec(), [("host", "q.test")]);

    state.proceed();
}

#[test]
fn proceed_with_amended_headers() {
    let scenario = Scenario::builder().get("https://q.test").build();

    let mut state = scenario.to_prepare();

    state.header("Cookie", "name=bar").unwrap();
    state.header("Cookie", "name2=baz").unwrap();

    let inner = state.inner();
    let request = inner.call.request();

    assert_eq!(
        request.headers_vec(),
        [
            //
            ("host", "q.test"),
            ("cookie", "name=bar"),
            ("cookie", "name2=baz"),
        ]
    );

    state.proceed();
}
