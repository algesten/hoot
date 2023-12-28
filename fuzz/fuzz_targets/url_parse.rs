#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(u) = hoot::Url::parse_str(s) else {
        return;
    };
    let _ = u.scheme();
    let _ = u.username();
    let _ = u.password();
    let _ = u.host();
    let _ = u.hostname();
    let _ = u.port();
    let _ = u.pathname();
    let _ = u.query();
    let _ = u.fragment();
});
