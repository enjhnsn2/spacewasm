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
}
