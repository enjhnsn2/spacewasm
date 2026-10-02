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

// Mutable counterpart of the above (same safety contract); gives
// `<InnerVec as DerefMut>::deref_mut` (and hence `Vec::deref_mut`) a known length.
#[flux_rs::extern_spec(core::slice)]
#[flux_rs::spec(fn(data: *mut[@p] T, len: usize) -> &mut [T][len]
    requires p.addr != 0
          && (len == 0 || (p.addr % T::align_of() == 0 && p.addr >= p.base && len * T::size_of() <= p.size)))]
unsafe fn from_raw_parts_mut<'a, T>(data: *mut T, len: usize) -> &'a mut [T];

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

// ---------------------------------------------------------------------------
// Detached refinements for `types::MemPageSize` / `types::MemType` (types.rs itself is
// untouched). `MemPageSize` is indexed by the page size in bytes and `MemType` by its page
// size (`page`); `memory::Memory` carries the same `page` so that its byte size is always
// `pages * page` for a `u32` page count, which is what makes `Memory::grow` monotone and
// `Memory::size` exact. The alignment is 16 for 64 KiB pages and 1 for byte pages, so the
// page layout `Layout::from_size_align(size, page_alignment)` is valid whenever
// `size + 15 <= isize::MAX`.
use crate::{MemPageSize, MemType};

#[flux_rs::specs {
    #[refined_by(size: int)]
    #[invariant(size == 1 || size == 65536)]
    enum MemPageSize {
        _1 -> MemPageSize[1],
        _65536 -> MemPageSize[65536],
    }

    impl MemPageSize {
        fn size(&MemPageSize[@p]) -> usize[p];
        fn alignment(&MemPageSize[@p]) -> usize{v: (p == 1 && v == 1) || (p == 65536 && v == 16)};
    }

    #[refined_by(page: int)]
    #[invariant(page == 1 || page == 65536)]
    struct MemType {
        initial_pages: u32,
        max_pages: Option<u32>,
        page_size: MemPageSize[page],
    }

    impl MemType {
        fn zero() -> MemType[65536];
        fn page_size(&MemType[@t]) -> usize[t.page];
        fn page_alignment(&MemType[@t]) -> usize{v: (t.page == 1 && v == 1) || (t.page == 65536 && v == 16)};
    }
}]
const _: () = ();

// `u64::from(u32)` is a lossless widening (flux-core only covers `TryFrom`); needed by
// `Memory::grow` to relate the new byte size to the page count.
#[flux_rs::extern_spec]
impl From<u32> for u64 {
    #[flux_rs::spec(fn(value: u32[@x]) -> u64[x])]
    fn from(value: u32) -> u64;
}

// Detached specs for `Engine` helpers defined in src/store.rs (wave 2, interpreter
// worker): they only swap the (unrefined) `memory`/`table` fields, so the engine's
// `sp`/`fp`/stack indices are preserved. Needed by `Interpreter::memory_grow`.
use crate::Engine;

// `Stack::new` (src/stack.rs) builds the buffer for exactly `size` words; `Engine::new`
// (src/store.rs) needs `size >= 0` for the engine invariant `sp <= size` with `sp == 0`.
use crate::{AllocError, Stack};

#[flux_rs::specs {
    impl Engine {
        fn clear_memory(self: &mut Engine[@e]);
        fn clear_table(self: &mut Engine[@e]);
    }

    impl Stack {
        fn new(size: usize) -> Result<Stack{s: s.size == size}, AllocError>;
    }
}]
const _: () = ();

// `str`: flux refines `str` by its contents (sort `str`) and provides the theory
// function `str_len`. flux-core has no `str` specs, so `len`/`as_bytes` are given
// here so that `Vec::new(s.len())` followed by a push per byte of `s.as_bytes()`
// verifies (`util::string::String::try_from(&str)`).
#[flux_rs::extern_spec]
impl str {
    #[flux_rs::spec(fn(&str[@s]) -> usize[str_len(s)])]
    fn len(&self) -> usize;

    #[flux_rs::spec(fn(&str[@s]) -> &[u8][str_len(s)])]
    fn as_bytes(&self) -> &[u8];
}


// ---------------------------------------------------------------------------
// Detached refinements for `visitor::ModuleRef` / `visitor::HostModuleRef` (visitor.rs is
// untouched): both are indexed by the module index they hold, so that `Store`'s accessors
// can require `r.idx < s.modules.len` (resp. `s.host.len`). `HostModuleRef::new` performs
// a narrowing cast, so its result is left unrefined.
use crate::{HostModuleRef, ModuleRef};

#[flux_rs::specs {
    #[refined_by(idx: int)]
    struct ModuleRef(u8[idx])

    #[refined_by(idx: int)]
    struct HostModuleRef(u8[idx])
}]
const _: () = ();

// `Step for u32`: flux-core only gives `usize`/`i32` the arithmetic `Step` refinement
// (everything else falls back to the uninterpreted defaults), so `for _ in 0..n` with a
// `u32` bound (e.g. `Reader::read_vec_in`) had an unknown trip count. Mirrors
// flux-core's `impl Step for usize {}` (requires `feature(step_trait)`, cfg'd on `flux`).
#[flux_rs::extern_spec(core::iter)]
#[flux_rs::assoc(
    fn step_forward(start: int, count: int) -> int { start + count }
    fn size(lo: int, hi: int) -> int { hi - lo }
)]
impl core::iter::Step for u32 {}

// ---------------------------------------------------------------------------
// Detached refinement for `imports::WasmRef` (wave 4, engine worker): a Wasm function
// reference carries the `ModuleRef` it points into, so `Engine::invoke` /
// `call_impl_enter_module` can require `f_ref.module.idx < e.store.modules.len`.
use crate::WasmRef;

#[flux_rs::specs {
    #[refined_by(module: ModuleRef, index: int)]
    struct WasmRef {
        module: ModuleRef[module],
        index: u16[index],
    }
}]
const _: () = ();

// Qualifier for the `Engine` invariant `size <= u32::MAX` (wave 4): a loop-head kvar can
// only express facts shaped like a qualifier, and the default set has no constant upper
// bound, so without this the invariant is lost across loops over a `&mut Engine`
// (`InterpreterRunner::run`).
flux_rs::defs! {
    qualifier StackSizeFitsU32(x: int) { x <= 4294967295 }
}

// ---------------------------------------------------------------------------
// Detached specs for `StaticVec` (wave 4): `push` succeeds exactly when there is room,
// so `sv.push(..).unwrap()` in `Interpreter::call_host_op` (at most
// `MAX_HOST_FUNCTION_PARAMS` pushes, one per parameter of a `HostValList`) verifies.
use crate::StaticVec;

// Detached specs for `util::StaticVec` (src/util/static_vec.rs is untouched): `len` is
// exact, `push` succeeds iff there is room (`len < N`) and then grows the length by one,
// `pop` succeeds iff non-empty and then shrinks it by one; `new`/`default` start empty.
// Needed by `text::TextBuilder` (control-frame and value-stack lengths).
use crate::util::StaticVec;

#[flux_rs::specs {
    impl StaticVec<T, N> {
        fn new() -> StaticVec<T, N>[0];
        fn push(self: &strg StaticVec<T, N>[@v], value: T) -> Result<(), AllocError>[v.len < N]
            ensures self: StaticVec<T, N>[if v.len < N { v.len + 1 } else { v.len }];
        fn len(&StaticVec<T, N>[@v]) -> usize[v.len];

        fn len(&StaticVec<T, N>[@v]) -> usize[v.len];
        fn push(self: &strg StaticVec<T, N>[@v], value: T) -> Result<(), AllocError>[v.len < N]
            ensures self: StaticVec<T, N>[if v.len < N { v.len + 1 } else { v.len }];
        fn pop(self: &strg StaticVec<T, N>[@v]) -> Option<T>[v.len > 0]
            ensures self: StaticVec<T, N>[if v.len > 0 { v.len - 1 } else { v.len }];
    }

    impl core::default::Default for StaticVec<T, N> {
        fn default() -> StaticVec<T, N>[0];
    }
}]
const _: () = ();


// `Clone for u16` preserves the value (same reason as `usize` above): `text::ControlFrame`
// derives `Clone` and its invariant bounds its `u16` `height` field.
#[flux_rs::extern_spec]
#[flux_rs::assoc(fn cloned(old: int, new: int) -> bool { old == new })]
impl Clone for u16 {
    #[flux_rs::spec(fn(&u16[@v]) -> u16[v])]
    fn clone(&self) -> u16;
}
