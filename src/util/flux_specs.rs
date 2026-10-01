//! Extern flux specifications for `core` items that flux-core does not cover yet.
#![allow(unused)]

use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

// `ManuallyDrop<T>` is a transparent wrapper: it is refined by the refinement of
// the wrapped value, and (de)referencing yields that same value.
#[flux_rs::extern_spec(core::mem)]
#[flux_rs::refined_by(inner: T)]
struct ManuallyDrop<T>;

#[flux_rs::extern_spec(core::mem)]
impl<T> ManuallyDrop<T> {
    #[flux_rs::spec(fn(value: T[@v]) -> ManuallyDrop<T>[v])]
    const fn new(value: T) -> ManuallyDrop<T>;

    #[flux_rs::spec(fn(slot: ManuallyDrop<T>[@v]) -> T[v])]
    const fn into_inner(slot: ManuallyDrop<T>) -> T;
}

#[flux_rs::extern_spec(core::mem)]
impl<T> Deref for ManuallyDrop<T> {
    #[flux_rs::spec(fn(&ManuallyDrop<T>[@v]) -> &T[v])]
    fn deref(&self) -> &T;
}

#[flux_rs::extern_spec(core::mem)]
impl<T> DerefMut for ManuallyDrop<T> {
    #[flux_rs::spec(fn(self: &mut ManuallyDrop<T>[@v]) -> &mut T[v])]
    fn deref_mut(&mut self) -> &mut T;
}


// ---------------------------------------------------------------------------
// `core::slice::from_raw_parts`: flux-core has no spec for it, so without one
// the resulting slice has an unknown length. Preconditions follow the Rust
// safety contract (non-null; for a non-empty slice also aligned and with
// `len * size_of::<T>()` bytes in bounds of the allocation), inlining
// flux-core's `valid`/`aligned_to`. Like flux-core's `ptr::read` for ZSTs, an
// empty slice only needs a non-null pointer (this is what `InnerVec`'s
// `cap == 0` invariant guarantees).
#[flux_rs::extern_spec(core::slice)]
#[flux_rs::spec(fn(data: *const[@p] T, len: usize) -> &[T][len]
    requires p.addr != 0
          && (len == 0 || (p.addr % T::align_of() == 0 && p.addr >= p.base && len * T::size_of() <= p.size)))]
unsafe fn from_raw_parts<'a, T>(data: *const T, len: usize) -> &'a [T];

// ---------------------------------------------------------------------------
// Detached specs for the (already verified) `InnerVec` accessors, so wrappers
// such as `Vec` can state their own specs in terms of `inner.len`/`inner.cap`.
use crate::util::InnerVec;

#[flux_rs::specs {
    impl InnerVec<T> {
        fn zero() -> InnerVec<T>[InnerVec { base: 0, addr: 0, size: 0, cap: 0, len: 0 }];
        fn len(&InnerVec<T>[@v]) -> usize[v.len];
        fn is_empty(&InnerVec<T>[@v]) -> bool[v.len == 0];
        fn capacity(&InnerVec<T>[@v]) -> usize[v.cap];
    }

    impl core::ops::Deref for InnerVec<T> {
        fn deref(&InnerVec<T>[@v]) -> &[T][v.len];
    }
}]
const _: () = ();
