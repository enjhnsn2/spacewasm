//! Extern flux specifications for `core` items that flux-core does not cover yet.
#![allow(unused)]

use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;

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

// Attaching slice metadata keeps the data pointer's allocation indices (as flux-core does
// for `NonNull::slice_from_raw_parts`); the raw-pointer form is a safe fn with no
// precondition.
#[flux_rs::extern_spec(core::ptr)]
#[flux_rs::spec(fn(data: *mut[@p] T, len: usize) -> *mut[p.base, p.addr, p.size] [T])]
fn slice_from_raw_parts_mut<T>(data: *mut T, len: usize) -> *mut [T];

// `core::slice::IterMut<'a, T>`: the mutable slice iterator, refined like flux-core's
// `Iter` by the current index and the slice length. `next` is given an explicit spec so
// that the yielded `&mut T` keeps `T`'s (existential) refinement: without it flux refines
// the item type with a fixed index, which forbids strong updates such as `Option::take`
// or `Option::replace` through the yielded reference.
#[flux_rs::extern_spec(core::slice)]
#[flux_rs::refined_by(idx: int, len: int)]
struct IterMut<'a, T>;

#[flux_rs::extern_spec(core::slice)]
#[flux_rs::assoc(
    fn size(x: IterMut) -> int { x.len - x.idx }
    fn done(x: IterMut) -> bool { x.idx >= x.len }
    fn step(x: IterMut, y: IterMut) -> bool {
        x.len == y.len &&
        (if x.idx < x.len { y.idx == x.idx + 1 } else { y.idx == x.idx })
    }
)]
impl<'a, T> Iterator for IterMut<'a, T> {
    #[flux_rs::spec(fn(self: &mut IterMut<T>[@curr_s]) -> Option<&mut T>[curr_s.idx < curr_s.len]
           ensures self: IterMut<T>{next_s:
               curr_s.len == next_s.len &&
               (curr_s.idx < curr_s.len => next_s.idx == curr_s.idx + 1) &&
               (curr_s.idx >= curr_s.len => next_s.idx == curr_s.idx)
           })]
    fn next(&mut self) -> Option<&'a mut T>;
}

#[flux_rs::extern_spec(core::slice)]
impl<T> [T] {
    #[flux_rs::spec(fn(&mut Self[@n]) -> IterMut<T>[0, n])]
    fn iter_mut(&mut self) -> IterMut<'_, T>;
}

// `Clone for usize` preserves the value: needed so that `#[derive(Clone)]` on a struct
// whose invariant relates `usize` fields (e.g. `paging::Page`) can re-establish it.
#[flux_rs::extern_spec]
#[flux_rs::assoc(fn cloned(old: int, new: int) -> bool { old == new })]
impl Clone for usize {
    #[flux_rs::spec(fn(&usize[@v]) -> usize[v])]
    fn clone(&self) -> usize;

// Flux leaves bitwise operators on integer-sorted values uninterpreted. This
// primop property axiomatizes the one fact we need about `&`: for non-negative
// operands, `x & y` is non-negative and bounded by both operands (true for any
// two's-complement width). Used by `text::JumpTarget::offset` (`addr & 0xFF < 256`).
flux_rs::defs! {
    property BitAndNonNegBound[&](x, y) {
        (0 <= x && 0 <= y) => (0 <= [&](x, y) && [&](x, y) <= x && [&](x, y) <= y)
    }
}
