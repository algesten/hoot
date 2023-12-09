use core::mem::align_of;
use core::mem::size_of;

use httparse::{Header, EMPTY_HEADER};

use crate::{HootError, Result};

// TODO: make this configurable.
const MAX_HEADERS: usize = 100;

/// Use a generic byte buffer to write httparse Header.
// TODO: Are these lifetimes ok?
pub(crate) fn cast_buf_for_headers<'a, 'b>(buf: &'a mut [u8]) -> Result<&'a mut [Header<'b>]> {
    let byte_len = buf.len();

    // The alignment of Header
    let align = align_of::<httparse::Header>();

    // Treat buffer as a pointer to some memory.
    let ptr = buf.as_mut_ptr() as *mut u8;

    // The amount of offset needed to be aligned.
    let offset = ptr.align_offset(align);

    if offset >= byte_len {
        return Err(HootError::InsufficientSpaceToParseHeaders);
    }

    // The number of Header elements we can fit in the buffer.
    let space_for = (byte_len - offset) / size_of::<httparse::Header>();

    // In case we got crazy big memory.
    let len = space_for.min(MAX_HEADERS);

    // Move pointer to alignment
    // SAFETY: We checked above that this is within bounds.
    let ptr = unsafe { ptr.add(offset) };

    // SAFETY: We checked alignment and how many headers we can fit once aligned.
    // TODO: I'm uncertain of my use of unsafe here.
    let header_buf = unsafe { core::slice::from_raw_parts_mut(ptr as *mut Header, len) };

    // SAFETY: ptr+len is not unitialized memory (since it came from a valid
    // &mut [u8] slice, however it also doesn't have correct data for Header.
    // This might be naive, but I think we can fill the space with valid values this.
    header_buf.fill(EMPTY_HEADER);

    Ok(header_buf)
}
