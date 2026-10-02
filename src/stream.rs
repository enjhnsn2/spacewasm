use core::ops::Deref;

use crate::util::InnerVec;

/// A borrowed view over a buffer owned by a [`WasmStream`].
///
/// Flux: refined by the wrapped buffer's `InnerVec` index, so that
/// `chunk.len()` and `chunk[i]` (both through `Deref`) agree on `inner.len`.
#[flux_rs::refined_by(inner: InnerVec)]
pub struct Chunk(#[flux_rs::field(InnerVec<u8>[inner])] InnerVec<u8>);

impl From<InnerVec<u8>> for Chunk {
    #[flux_rs::spec(fn(value: InnerVec<u8>[@v]) -> Chunk[v])]
    fn from(value: InnerVec<u8>) -> Self {
        Chunk(value)
    }
}

impl Chunk {
    /// Hand the buffer back to `stream`, leaving an empty chunk behind.
    #[flux_rs::spec(fn(self: &strg Self, stream: _) ensures self: Self)]
    pub(crate) fn return_(&mut self, stream: &mut dyn WasmStream) {
        let to_drop = core::mem::replace(&mut self.0, InnerVec::zero());
        stream.return_(to_drop)
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        debug_assert_eq!(
            self.0.capacity, 0,
            "Chunk dropped without returning its buffer to the stream"
        );
    }
}

impl Deref for Chunk {
    type Target = [u8];
    #[flux_rs::spec(fn(&Self[@c]) -> &[u8][c.inner.len])]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

pub trait WasmStream {
    /// Read the next chunk of data from the data 'feeder'
    /// Returns Ok(None) when we are finished reading
    /// Returns Err(code) to indicate an error code while reading this chunk
    fn read(&mut self) -> Result<Option<InnerVec<u8>>, u8>;

    /// Returns a buffer back to the stream so that the memory may be reused.
    fn return_(&mut self, chunk: InnerVec<u8>);
}
