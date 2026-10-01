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


// Attaching slice metadata keeps the data pointer's allocation indices (as flux-core does
// for `NonNull::slice_from_raw_parts`); the raw-pointer form is a safe fn with no
// precondition.
#[flux_rs::extern_spec(core::ptr)]
#[flux_rs::spec(fn(data: *mut[@p] T, len: usize) -> *mut[p.base, p.addr, p.size] [T])]
fn slice_from_raw_parts_mut<T>(data: *mut T, len: usize) -> *mut [T];
