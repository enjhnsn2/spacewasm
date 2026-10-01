# ABORT log — spacewasm

## type 3 util::rc::RcInner (src/util/rc.rs:15)

Error: `arithmetic operation may underflow` at src/util/rc.rs:326 (`RcInner::dec`: `count - 1`).
The function relies on `hint::assert_unchecked(count != 0)`, which flux does not interpret.

Strategies tried:
1. Precondition on `dec`/`inc`: they take `&self` and the count lives in a `Cell<u32>`, so there is no
   refinement index to constrain (`fn(&Self) -> usize` cannot mention the count).
2. `refined_by(count: int)` on `RcInner`: needs `#[field(Cell<u32>[count])]`, but flux-core has no
   refinement for `core::cell::Cell` and interior mutability (`set` through `&self`) cannot be tracked.
3. Spec on the callee `hint::assert_unchecked`: a spec `fn(bool[true])` would only move the obligation
   to the caller, which still cannot prove `count != 0`.

Outcome: no annotation added; `RcInner` has no expressible invariant. The `count != 0` fact belongs to the
`Rc` ownership discipline (every live `Rc` holds a count), which is a temporal property outside flux's model.

## Policy note: stateless types

Unit structs and plain enums carry no refinable state. When their `--check-type` run is clean they are
checkpointed as `DONE (no invariant)`. When the only errors are inside their associated functions and
depend on a *different* type's refinement (typically `reader::Reader`, type uid 94), they are logged here
as deferred and revisited in Phase 2, since no `refined_by`/`invariant` on the type itself can help.

## type 7 module::CustomSection (src/module.rs:481) — deferred to Phase 2

Error: `arithmetic operation may underflow` at src/module.rs:498 (`wasm.offset() - start` in `CustomSection::read`).
`CustomSection` is a unit struct. The fact needed is that `Reader::offset()` is monotonic across reads,
which is a `Reader` refinement (type uid 94) plus specs on `Reader::read_*`. Nothing on this type can express it.

## type 2 util::vec::IntoIter (src/util/vec.rs:383)

Errors: `next`/`next_back` fail the flux-core `Iterator::next` spec (internal error infer.rs:887 on an
unrefined iterator), `offset(-1)`/`read` pointer preconditions, and `size_hint` (`end as usize - start as usize`
underflow and division by `size_of::<T>()`).

Strategies tried:
1. Refinement `refined_by(base, buf, size, cap, start, rem)` with `end` indexed as `start + rem * size_of`
   and Iterator assoc refinements: flux rejects it (`rem` is not determined by any field index position,
   since it only appears inside an arithmetic expression), and a full sweep with it present made cvc5
   diverge (nonlinear `rem * size_of` with `%` alignment terms).
2. `size_hint` casts pointers to `usize`; this flux revision does not preserve addresses through
   pointer-to-integer casts, so the underflow and the division by zero (real for ZST `T`) cannot be discharged
   by any spec, and `Iterator::size_hint` is a trait method that cannot take a precondition.
3. `Drop` needs `Layout::array::<T>(cap).unwrap()` to be `Ok`, which needs flux-core's `is_valid_layout`
   predicate, not nameable from this crate.

Outcome: no annotation kept. Revisit `next`/`next_back` in Phase 2 by refining `end` by its own address and
stating `end == start + rem * size_of` as an invariant rather than as the field index.

## Triage pass (types 18-114): clean types

66 types verified clean without annotations and checkpointed as DONE (no invariant).
Candidates with a plausible invariant to add in Phase 2 if a caller needs it: 47 Limit (min<=max); 86 MemType (limits); 87 TableType (limits); 57 JumpTarget (page/offset bounds); 67 Chunk (expose InnerVec len); 71 String (expose len).

## Allocator FFI boundary — decision for all allocation-owning types (wave 1, w1)

`crate::util::alloc::Allocator::alloc` now carries a trait-method spec (src/util/alloc.rs):

    fn(&Self, layout: Layout[@l])
      -> Result<*mut{p: p.addr != 0 && p.base == p.addr && p.size == l.size && p.addr % l.align == 0} u8, AllocError>

i.e. an `Ok` pointer is the start of a block tracked as exactly `layout.size()` bytes (the same
convention flux-core uses for `core::alloc::Allocator::allocate` / `layout_fits`) and aligned to
`layout.align()`. `dealloc` is left unspecified (no precondition), so `Drop` impls that compute the
layout with the unspecced `Layout::for_value` are not burdened. Call sites use it as:
`alloc.alloc(Layout::new::<T>())? as *mut T` → `ptr::write(ptr, v)` verifies (`Box::new_in`).

Decision: option (a). Route (b) was tried and is closed: a `#[flux_rs::spec]` on the
`extern "C" fn __spacewasm_alloc` declaration is silently ignored (flux-driver's collector only
visits `ItemKind::Fn`/impl/trait items, never `ForeignItem`s), so the FFI contract cannot be stated
where it belongs.

### `<GlobalAllocator as Allocator>::alloc` (src/util/alloc.rs:154) — ABORT, needs trust at the FFI boundary

Error: postcondition cannot be proved for `p.base == p.addr`, `p.size == l.size`,
`p.addr % l.align == 0` (3 errors; `p.addr != 0` is proved from the `is_null` test).
The pointer comes from the unrefined foreign call; nothing in the body can establish the block's
extent or alignment. Tried: (i) spec on the foreign declaration (ignored, see above); (ii) weaker
trait spec — anything weaker than "block of `l.size` bytes aligned to `l.align`" makes
`ptr::write` in `Box::new_in` unprovable, so there is no useful middle ground. The honest fix is
`#[flux_rs::trusted]` on this one impl method (not added, per the rules).

### Cross-cutting notes for other workers (Vec, Stack, PageAllocator, ...)

- Inherited trait specs are NOT checked reliably in this flux revision: even the pure forwarding
  impl `impl<T: Allocator> Allocator for &T` failed 3/4 conjuncts of the inherited spec, and passed
  as soon as the identical spec was restated with `#[flux_rs::spec]` on the impl method. Every impl
  of `Allocator::alloc` should restate the trait spec verbatim (done for `&T` and `GlobalAllocator`).
  CONSEQUENCE FOR uid 95 (`PageAllocator`, wave 3, src/util/paging.rs:81): its
  `impl Allocator for PageAllocator<A, MAX_PAGES>` now reports 4 "postcondition cannot be proved"
  errors (all four conjuncts) because it forwards to the unspecced `PageAllocatorInner::alloc`
  (uid 72, which itself is RESULT=TIMEOUT on its own). Fix on that side: give
  `PageAllocatorInner::alloc` a spec ensuring the same four facts about its `Ok` pointer (its pages
  come from `A::alloc`, so the block facts are available) and restate the trait spec on the
  `PageAllocator` impl method.
- `T::size_of()` / `T::align_of()` are not resolvable on a `?Sized` type parameter
  ("associated refinement not found"), so a `Box<T: ?Sized>`-style struct cannot state pointee-size
  facts in its invariant; they have to be re-established at `Sized` use sites.
- `T::size_of() >= 0` has no axiom, but it folds to a constant for any concrete type (flux
  evaluates `size_of`/`align_of` via `layout_of`), so `requires A::size_of() >= 0` on a generic fn
  is discharged for free by callers that fix `A` (e.g. `Rc::new` → `Rc::new_in::<_, GlobalAllocator>`).
- `&raw mut place` / `&x as *const T` yields `*mut{base == addr && addr != 0 && size == T::size_of()
  && aligned}` for `Sized` pointees and only `{base == addr && addr != 0}` for `?Sized` ones
  (flux-refineck `raw_ptr_with_size`). Hence `ptr::read(&b.field)` needs `Field::size_of() >= 0`.
- New extern spec in src/util/flux_specs.rs: `core::ptr::slice_from_raw_parts_mut(data, len)`
  returns `*mut[p.base, p.addr, p.size] [T]` (indices of `data` carried through, no precondition).
  An extern spec for `impl<T: ?Sized> From<&mut T> for NonNull<T>` is NOT expressible (its elided
  impl lifetime cannot be written in an extern spec: E0637 elided / "generic parameters don't match"
  named), so `NonNull::from(&mut x)` / `.into()` stays unrefined beyond NonNull's own `addr != 0`.

## type 26 util::box_::Box (src/util/box_.rs:15) — ABORT (5 errors left in box_.rs + 2 deferred in host.rs)

Refinement kept: `refined_by(base, addr, size)`, `invariant(addr != 0 && size >= 0 && (size == 0 || addr >= base))`
(a zero-sized block is `NonNull::dangling()`, whose `base` is unconstrained by flux-core, so
`addr >= base` is guarded). Verified: `new`, `new_in` (via the `Allocator::alloc` spec), `from_raw`
(requires = invariant), `as_ptr`/`as_mut_ptr` (expose the indices), `into_raw_with_allocator`,
`leak`, `deref`/`deref_mut`, `drop`, `eq`/`cmp`/`partial_cmp`, `From<Vec>`, `IntoIterator`, `fmt`,
`try_clone` (sized). `into_raw_with_allocator`/`leak`/`into_inner` carry `requires A::size_of() >= 0`
(needed by `ptr::read(&b.alloc)`, see cross-cutting notes; `Rc::new_in` propagates it, `Rc::new`
discharges it).

Remaining errors and why:
1. `<Box<T, A> as Clone>::clone` (box_.rs:50) and `<Box<[T], A> as Clone>::clone` (box_.rs:174):
   `try_clone().expect(..)` — flux-core's `Result::expect` requires `Result[true]`, but allocation can
   genuinely fail and the panic is documented behaviour. A trait method cannot take a precondition and
   `try_clone` cannot promise `Ok`. Not hidden behind a `match`/`unwrap_or_else` rewrite on purpose:
   that would only remove the check, not the panic.
2. `Box::<T, A>::into_inner` (box_.rs:121, 2 errors): `ptr::read(ptr)` needs `T::size_of() <= size`
   and alignment, which cannot live in the invariant of a `?Sized` struct (see notes), and the
   pointer returned by `into_raw_with_allocator` (`&raw mut **b`, `?Sized` impl) carries no size.
   Tried: (i) invariant with `T::size_of()` — rejected ("associated refinement not found");
   (ii) spec on `into_raw_with_allocator` returning the Box's own indices — not provable from
   `&raw mut **b` without rewriting it to `b.ptr`, and the indices would still lack `size_of`;
   (iii) reading through `&*b` instead (Sized place) would need both a reorder and
   `requires T::size_of() >= 0`. No callers in the crate.
3. `Box::<[T], A>::try_clone` (box_.rs:165): `self[i]` out of bounds — `self.len()` and `self[i]`
   each go through `Deref` separately and produce unrelated existential slice lengths. The
   `Deref` spec cannot tie the slice length to the Box (the fat-pointer metadata is not part of
   flux's raw-pointer refinement). A semantics-preserving rewrite (`let src: &[T] = self;` then use
   `src.len()`/`src[i]`) would verify; left for Phase 2 since the surrounding `clone` is error 1.
4. src/host.rs:150 and :321 (`into_global_value_dyn`, `into_host_function_dyn`): internal flux
   error `checker.rs:1877` on the `*mut T as *mut dyn Trait` unsize cast — tool limitation, deferred.

## type 68 util::rc::Rc (src/util/rc.rs:20) — ABORT (TIMEOUT in one fn; 1 pre-existing error from type 3)

Refinement kept: `refined_by(base, addr, size)` on the `NonNull<RcInner<T>>` field,
`invariant(addr != 0)`; `from_inner_in` propagates the pointer's indices; `new_in` requires
`A::size_of() >= 0` (for `Box::leak`); the slice constructors (`new_slice*`, `new_uninit_slice_in`)
require `T::size_of() >= 0`, and `new_uninit_slice_in` promises
`*mut{p: p.addr > 0 && p.addr >= p.base && p.size >= 0 && len * T::size_of() <= p.size && p.addr % T::align_of() == 0} T`,
against which `new_slice_in`'s `ptr::write(slice_ptr.add(i), ..)` loop verifies.
Verified individually: `new`, `new_in`, `new_slice`, `new_slice_in`, `new_slice_with_default`,
`new_slice_with_default_in` (needs `opts(solver = "z3")`: cvc5 exceeds 30s, z3 8s), `clone`,
`is_unique`, `get_mut`, `inner`, `from_inner_in`, `deref`, `drop`, `fmt`.

Remaining:
1. `Rc::<[T], A>::new_uninit_slice_in` (rc.rs:~100): RESULT=TIMEOUT with cvc5 and with
   `opts(solver = "z3")`; a direct `cargo flux --only-check` run did not finish in 230s with either,
   so it is a divergence, not a slow solve. The body chains `Layout::new`, `Layout::array`
   (flux-core: `pow2` bitvector invariant plus `len * T::size_of()`), `extend`, `pad_to_align`
   (both unspecced), `alloc`, `ptr::write`, `ptr.add(slice_offset)`, `slice_from_raw_parts_mut`,
   `NonNull::new_unchecked`. Even if it terminated, `Layout::extend`/`pad_to_align` need extern
   specs whose offset/size relation is cross-component (`o + s2 <= l.size`) and not expressible
   without an exact `round_up` formula (nonlinear div/mod). Its spec is kept so callers verify.
2. `RcInner::dec` underflow (rc.rs:351) — this is type 3's ABORT (see above), swept into the
   `--check-type=68` prefix match (`util::rc::Rc` is a prefix of `util::rc::RcInner`).
