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


// Flux leaves bitwise operators on integer-sorted values uninterpreted. This
// primop property axiomatizes the one fact we need about `&`: for non-negative
// operands, `x & y` is non-negative and bounded by both operands (true for any
// two's-complement width). Used by `text::JumpTarget::offset` (`addr & 0xFF < 256`).
flux_rs::defs! {
    property BitAndNonNegBound[&](x, y) {
        (0 <= x && 0 <= y) => (0 <= [&](x, y) && [&](x, y) <= x && [&](x, y) <= y)
    }
}
