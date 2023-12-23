use core::fmt;
use core::mem;
use core::str;
use httparse::Header as InnerHeader;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Header<'a> {
    name: &'a str,
    value: &'a [u8],
}

impl<'a> Header<'a> {
    #[inline(always)]
    pub fn name(&self) -> &str {
        self.name
    }

    #[inline(always)]
    pub fn try_value(&self) -> Option<&str> {
        str::from_utf8(self.value).ok()
    }

    #[inline(always)]
    pub fn value(&self) -> &str {
        self.try_value().expect("header value to be valid utf-8")
    }

    #[inline(always)]
    pub fn value_raw(&self) -> &[u8] {
        self.value
    }
}

impl<'a> fmt::Debug for Header<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Header");
        f.field("name", &self.name);
        if let Some(value) = self.try_value() {
            f.field("value", &value);
        } else {
            f.field("value", &self.value);
        }
        f.finish()
    }
}

pub(crate) fn transmute_headers<'a, 'b>(headers: &'b [InnerHeader<'a>]) -> &'b [Header<'a>] {
    // SAFETY: Our goal is to have hoot::Header be structurally the same
    // as httparse::Header. This is asserted by the test below.
    unsafe { mem::transmute(headers) }
}

#[cfg(test)]
mod test {
    use super::*;
    use memoffset::offset_of;

    #[test]
    fn assert_httparse_header_transmutability() {
        assert_eq!(mem::size_of::<Header>(), mem::size_of::<InnerHeader>());
        assert_eq!(mem::align_of::<Header>(), mem::align_of::<InnerHeader>());
        assert_eq!(offset_of!(Header, name), offset_of!(InnerHeader, name));
        assert_eq!(offset_of!(Header, value), offset_of!(InnerHeader, value));
    }
}
