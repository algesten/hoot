use core::mem::align_of;
use core::mem::size_of;

use httparse::Header;

use crate::{HootError, Result};

/// Use a generic byte buffer to write httparse Header.
pub(crate) fn cast_buf_for_headers<'a, 'b>(buf: &'a mut [u8]) -> Result<&'a mut [Header<'b>]> {
    let byte_len = buf.len();

    // The alignment of Header
    let align = align_of::<httparse::Header>();

    // Treat buffer as a pointer to Header
    let ptr = buf.as_mut_ptr() as *mut Header;

    // The amount of offset needed to be aligned.
    let offset = ptr.align_offset(align);

    if offset >= byte_len {
        return Err(HootError::InsufficientSpaceToParseHeaders);
    }

    // The number of Header elements we can fit in the buffer.
    let len = (byte_len - offset) / size_of::<httparse::Header>();

    // Move pointer to alignment
    // SAFETY: We checked above that this is within bounds.
    let ptr = unsafe { ptr.add(offset) };

    // SAFETY: We checked alignment and how many headers we can fit once aligned.
    // MA: I'm uncertain of my use of unsafe here.
    let header_buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };

    Ok(header_buf)
}
