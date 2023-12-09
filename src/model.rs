use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;

use crate::out::Out;
use crate::vars::private;

use crate::{state::*, HootError};
use private::*;

pub struct CallState<S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    _state: PhantomData<S>,
    _version: PhantomData<V>,
    _method: PhantomData<M>,
    _btype: PhantomData<B>,
    pub(crate) send_byte_checker: Option<SendByteChecker>,
}

pub(crate) struct SendByteChecker {
    sent: u64,
    expected: u64,
}

impl SendByteChecker {
    pub(crate) fn new(expected: u64) -> Self {
        SendByteChecker { sent: 0, expected }
    }

    pub(crate) fn append(&mut self, sent: usize) -> Result<(), HootError> {
        let new_total = self.sent + sent as u64;
        if new_total > self.expected {
            return Err(HootError::SentMoreThanContentLength);
        }
        self.sent = new_total;
        Ok(())
    }

    pub(crate) fn assert_expected(&self) -> Result<(), HootError> {
        if self.sent != self.expected {
            return Err(HootError::SentLessThanContentLength);
        }

        Ok(())
    }
}

// #[derive(Copy, Clone, Debug, PartialEq, Eq)]
// pub(crate) enum BodyTypeRecv {
//     NoBody,
//     LengthDelimited(u64),
//     Chunked,
//     CloseDelimited,
// }

impl CallState<(), (), (), ()> {
    fn new<S: State, V: Version, M: Method, B: BodyType>() -> CallState<S, V, M, B> {
        CallState {
            _state: PhantomData,
            _version: PhantomData,
            _method: PhantomData,
            _btype: PhantomData,
            send_byte_checker: None,
        }
    }
}

pub struct Call<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub(crate) state: CallState<S, V, M, B>,
    pub(crate) out: Out<'a>,
}

impl<'a> Call<'a, (), (), (), ()> {
    pub fn new(buf: &'a mut [u8]) -> Call<'a, INIT, (), (), ()> {
        Call {
            state: CallState::new(),
            out: Out::wrap(buf),
        }
    }
}

impl<'a, S, V, M, B> Call<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub fn flush(self) -> Output<'a, S, V, M, B> {
        Output {
            state: self.state,
            output: self.out.flush(),
        }
    }

    pub fn resume(state: CallState<S, V, M, B>, buf: &'a mut [u8]) -> Call<'a, S, V, M, B> {
        Call {
            state,
            out: Out::wrap(buf),
        }
    }

    pub(crate) fn transition<S2: State, V2: Version, M2: Method, B2: BodyType>(
        self,
    ) -> Call<'a, S2, V2, M2, B2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }
}

pub struct Output<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub(crate) state: CallState<S, V, M, B>,
    pub(crate) output: &'a [u8],
}

impl<'a, S, V, M, B> Output<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    pub fn ready(self) -> CallState<S, V, M, B> {
        self.state
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.output
    }
}

impl<'a, S, V, M, B> Deref for Output<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl<'a, S, V, M, B> std::fmt::Debug for Call<'a, S, V, M, B>
    where
        S: State,
        V: Version,
        M: Method,
        B: BodyType,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Call").finish()
        }
    }
}
