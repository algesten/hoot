use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;

use crate::out::Out;
use crate::vars::private;

use crate::state::*;
use private::*;

pub struct CallState<S, V, M>(PhantomData<S>, PhantomData<V>, PhantomData<M>)
where
    S: State,
    V: Version,
    M: Method;

impl CallState<(), (), ()> {
    fn new<S: State, V: Version, M: Method>() -> CallState<S, V, M> {
        CallState(PhantomData, PhantomData, PhantomData)
    }
}

pub struct Call<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    pub(crate) state: CallState<S, V, M>,
    pub(crate) out: Out<'a>,
}

impl<'a> Call<'a, (), (), ()> {
    pub fn new(buf: &'a mut [u8]) -> Call<'a, INIT, (), ()> {
        Call {
            state: CallState::new(),
            out: Out::wrap(buf),
        }
    }
}

impl<'a, S, V, M> Call<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    pub fn flush(self) -> Output<'a, S, V, M> {
        Output {
            state: self.state,
            output: self.out.flush(),
        }
    }

    pub fn resume(state: CallState<S, V, M>, buf: &'a mut [u8]) -> Call<'a, S, V, M> {
        Call {
            state,
            out: Out::wrap(buf),
        }
    }

    pub(crate) fn transition<S2: State, V2: Version, M2: Method>(self) -> Call<'a, S2, V2, M2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }
}

pub struct Output<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    pub(crate) state: CallState<S, V, M>,
    pub(crate) output: &'a [u8],
}

impl<'a, S, V, M> Deref for Output<'a, S, V, M>
where
    S: State,
    V: Version,
    M: Method,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}
