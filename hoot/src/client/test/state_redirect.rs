use http::{Response, StatusCode};

use super::scenario::Scenario;

#[test]
fn without_recv_body() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "https://b.test")
        .build();

    scenario.to_redirect();
}

#[test]
fn with_recv_body() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "https://b.test")
        .recv_body(b"hi there", false)
        .build();

    scenario.to_redirect();
}

#[test]
fn absolute_url() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "https://b.test")
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap();

    assert_eq!(&state.uri().to_string(), "https://b.test/");
}

#[test]
fn relative_url_absolute_path() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "/foo.html")
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap();

    assert_eq!(&state.uri().to_string(), "https://a.test/foo.html");
}

#[test]
fn relative_url_relative_path() {
    let scenario = Scenario::builder()
        .get("https://a.test/x/foo.html")
        .redirect(StatusCode::FOUND, "y/bar.html")
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap();

    assert_eq!(&state.uri().to_string(), "https://a.test/x/y/bar.html");
}

#[test]
fn last_location_header() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .response(
            Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header("location", "https://b.test")
                .header("location", "https://c.test")
                .header("location", "https://d.test")
                .header("location", "https://e.test")
                .body(())
                .unwrap(),
        )
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap();

    assert_eq!(&state.uri().to_string(), "https://e.test/");
}
