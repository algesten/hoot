use core::fmt;
use core::mem;
use httparse::{Header, EMPTY_HEADER};

use crate::{HootError, Result};

// TODO: make this configurable.
const MAX_HEADERS: usize = 100;

/// Use a generic byte buffer to write httparse Header.
pub(crate) fn cast_buf_for_headers<'a, 'b>(buf: &'a mut [u8]) -> &'a mut [Header<'b>] {
    // SAFETY: align_to_mut docs say "This method is essentially a transmute with
    // respect to the elements in the returned middle slice". Transmute further
    // says that "..the result must be _valid_ at their given type". The "valid"
    // word means that we can't have data, even temporarily, that is not correct.
    //
    // Header contains a &str and &[u8] and &str must never be made to exist while
    // pointing to garbage data (which the incoming buf might have).
    //
    // This situation is similar to the example "Initializing an array element-by-element"
    // https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#initializing-an-array-element-by-element
    //
    // By transmuting to MaybeUninit<Header<'b>>, we can initialize the data safely
    // before transmuting to our final result.
    let (_, mut headers, _) = unsafe { buf.align_to_mut::<mem::MaybeUninit<Header<'b>>>() };

    if headers.len() > MAX_HEADERS {
        let max = headers.len().min(MAX_HEADERS);
        headers = &mut headers[..max];
    }

    // This is the point of using MaybeUninit.
    for header in &mut *headers {
        header.write(EMPTY_HEADER);
    }

    // SAFETY: See above rust doc link.
    unsafe { mem::transmute(headers) }
}

pub(crate) fn compare_lowercase_ascii(a: &str, lowercased: &str) -> bool {
    if a.len() != lowercased.len() {
        return false;
    }

    for (a, b) in a.chars().zip(lowercased.chars()) {
        if !a.is_ascii() {
            return false;
        }
        let norm = a.to_ascii_lowercase();
        if norm != b {
            return false;
        }
    }

    true
}

pub(crate) struct LengthChecker {
    handled: u64,
    expected: u64,
}

impl LengthChecker {
    pub fn new(expected: u64) -> Self {
        LengthChecker {
            handled: 0,
            expected,
        }
    }

    pub fn append(&mut self, amount: usize, err: HootError) -> Result<()> {
        let new_total = self.handled + amount as u64;
        if new_total > self.expected {
            return Err(err);
        }
        self.handled = new_total;
        Ok(())
    }

    pub fn assert_expected(&self, err: HootError) -> Result<()> {
        if self.handled != self.expected {
            return Err(err);
        }
        Ok(())
    }

    pub fn complete(&self) -> bool {
        self.handled == self.expected
    }
}

impl fmt::Debug for LengthChecker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LengthChecker")
            .field("handled", &self.handled)
            .field("expected", &self.expected)
            .finish()
    }
}
