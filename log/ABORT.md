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

## type 22 stack::Stack (src/stack.rs:4) — partial: all accessors verified, `new`/`drop` blocked

Refinement kept: `refined_by(base: int, addr: int, bytes: int, size: int)`, fields
`ptr: *mut[base, addr, bytes] u32`, `size: usize[size]`, invariants `size * 4 <= bytes` and
`addr >= base && bytes >= 0 && addr > 0 && addr % 4 == 0`. Specs: `len -> usize[s.size]`,
`read_u32/read_f32/write_u32/write_f32` require `addr < s.size`, `read_u64/read_f64/write_u64/write_f64`
require `addr + 2 <= s.size`. 18 -> 7 errors; every read/write/len verifies (the literal `4` is fine:
flux relates `u32::size_of()` to 4 for the `ptr.add`/deref obligations).

Remaining 7 errors, all in `Stack::new` (src/stack.rs:23-30, 26) and `<Stack as Drop>::drop` (src/stack.rs:111):
- `new`: `Layout::from_size_align(bytes, 4).unwrap()` needs flux-core's `is_valid_layout(bytes, 4)`
  (= `bytes + 3 <= isize::MAX && pow2(4)`); `bytes = size.checked_mul(4)?` only gives `bytes <= usize::MAX`,
  so the unwrap is a genuine (theoretical) panic for `size > isize::MAX / 4` and no spec can discharge it
  without a code change. `drop` has the same `unwrap` on `self.size * 4`.
- `new`: the 5 struct-invariant obligations (`size*4 <= bytes`, `addr >= base`, `bytes >= 0`, `addr > 0`,
  `addr % 4 == 0`) need `Allocator::alloc` to return `*mut[b, a, s] u8` with `s >= layout.size`,
  `a >= b`, `a > 0`, `a % layout.align == 0`. `Allocator::alloc` (src/util/alloc.rs) has no spec; worker 1 owns
  that file and the FFI allocator pattern. Once `alloc` is specified these 5 should discharge with no change here.
Strategies: (i) function spec: `new` has no inputs to constrain (the layout bound is on `size`, and adding
`requires size * 4 + 3 <= isize::MAX` would push an unprovable precondition onto `Engine::new`); (ii) struct
refinement is already as strong as the accessors need; (iii) the missing callee specs (`alloc`, `is_valid_layout`)
are outside this crate / this worker's files.

## type 21 interpreter::Interpreter (src/interpreter.rs:228) — blocked on Engine refinement + trait-level specs

`Interpreter` is a unit struct. All interpreter state (`sp: usize`, `fp: u32`, `stack: Stack`) lives in
`engine::Engine` (src/engine.rs, uid 114, wave 5), and the 172 functions in this check are 151
`<Interpreter as BaseVisitor>` + 20 `<Interpreter as IrVisitor>` trait-impl methods (`fn op(&self, state: &mut Self::State)`)
plus the inherent `Interpreter::br_impl`. Nothing on `Interpreter` itself can be refined; no annotation was kept in
src/interpreter.rs.

Error counts (`cargo flux -p spacewasm --only-check='def:interpreter::Interpreter'`):
- baseline (triage): 417 = 394 underflow + 23 other.
- after the Stack specs from this wave: 838 = 393 `arithmetic operation may underflow` (every `state.sp - n` /
  `state.sp -= n` in the `instruction!` macro arms at lines ~430-550 and the hand-written ops), 425 Stack
  read/write preconditions (`addr < s.size` / `addr + 2 <= s.size`; the 4 `unwrap`s are `StaticVec::push(..).unwrap()`
  in `call_host`), 19 out-of-bounds slice indexes (`store.modules()[..]`, `m.functions[..]`, `m.types[..]`,
  `host_modules()[..]`, `m.globals[..]` in `call_indirect`/`call_host`/`global_*`), 1 division by zero
  (`i32_div_s`, line 870: `a / b` after `b == 0` early-return; the `u32 -> i32` cast loses the guard).
  The +421 is expected: `Engine` is unrefined, so `state.sp` is unconstrained at every access.

Experiments (all reverted; src/engine.rs and src/visitor.rs are untouched in this branch):
1. Refining `Engine` with `refined_by(base: int, addr: int, bytes: int, size: int, sp: int, fp: int)`,
   `invariant(fp <= sp && sp <= size)`, `field(u32[fp]) fp`, `field(usize[sp]) sp`,
   `field(Stack[base, addr, bytes, size]) stack`: 838 -> 627 = 390 underflow + 161 `type invariant may not hold
   (when place is folded)` (each `state.sp -= n` / `+= n`) + 54 Stack preconditions (all push sites
   `write_*(state.sp, ..)`, which need `sp < size`, i.e. headroom) + 19 OOB + 1 div + 2.
2. Impl-level precondition on one method (`#[spec(fn(&Self, state: &strg Self::State[@e]) -> Result<(), Self::Error>
   requires e.sp >= 2 ensures state: Self::State{v: v.size == e.size})]`): the body verifies, but flux checks the
   impl spec against the trait method's unrefined signature and reports `a precondition cannot be proved` at the
   `fn` line. Writing `&strg Engine[@e]` instead is rejected structurally (`expected a refinement of
   &mut <Interpreter as BaseVisitor>::State`). So impl methods cannot carry the operand-stack-depth precondition.
3. Trait-level route (src/visitor.rs, outside this worker's files): `#[flux_rs::assoc(fn has_depth(s: Self::State, n: int) -> bool)]`
   on `BaseVisitor`, the trait method `#[spec(fn(&Self, state: &strg Self::State[@s]) -> Result<(), Self::Error>
   requires <Self as BaseVisitor>::has_depth(s, 2) ensures state: Self::State)]`, and on the impl
   `#[flux_rs::assoc(fn has_depth(s: Engine, n: int) -> bool { s.sp >= n })]` plus the impl spec from (2):
   accepted by this flux revision; `i32_wrap_i64`'s underflow and Stack-precondition errors are discharged and only
   the `fp <= sp` fold remains (fixable by defining `has_depth` as `s.sp >= s.fp + n`). This is the viable route.

Why ABORT here: every remaining error needs (a) the `Engine` refinement above (uid 114, src/engine.rs) and (b)
per-method `requires`/`ensures` on the `BaseVisitor`/`IrVisitor` trait methods in src/visitor.rs expressed through
assoc refinements over `Self::State` (depth `sp >= fp + n_pop` and headroom `sp + n_push <= size`), which the wasm
validator guarantees but which is not a local fact. Both files are outside this assignment; the macro-generated
`visit_fn!` signatures in visitor.rs would need per-method specs (the macro has no spec slot). The 19 OOB errors
additionally need `Store`/`Module` refinements (wave 4) relating `ModuleRef`/`FuncIdx`/`TypeIdx` to vector lengths.
`br_impl` (the only inherent method) also needs `LabelTarget::depth()`/`arity()` to be refined (uid 55/56, currently
unrefined) before a precondition can be stated.
Strategies: (i) function spec — impossible on trait impls (exp. 2) and `br_impl`'s inputs are unrefined;
(ii) struct refinement — `Interpreter` has no fields; the needed one is on `Engine` (exp. 1 measures it);
(iii) callee spec — the Stack specs are in place; no further callee in this worker's files helps.

## type 66 util::vec::Vec (src/util/vec.rs:15) — partial (wave 1, worker 2)

Refinement kept: `refined_by(inner: InnerVec)` with `#[field(InnerVec<T>[inner])]` and invariant
`inner.cap == 0 || T::size_of() == 0 || inner.cap * T::size_of() + T::align_of() - 1 <= isize::MAX`
(= flux-core's `is_valid_array_layout`, inlined; it lets `Drop` discharge `Layout::array(cap).unwrap()`).
Specs verified on: `new_from`, `new_from_with_alloc`, `new`, `zero`, `default`, `len`, `is_empty`, `capacity`,
`push`, `try_push`, `pop`, `assume_init`, `try_clone`, `deref`, `deref_mut`; `drop`, `eq`, `fmt`, `iter`, `iter_mut`
verify with no spec. `InnerVec::{zero,len,is_empty,capacity}` and `<InnerVec as Deref>::deref` got detached specs
in src/util/flux_specs.rs (plus an extern spec for `core::slice::from_raw_parts`) so `Vec` can speak about
`inner.len`/`inner.cap`.

Remaining 9 errors in 6 functions (all blocked outside src/util/vec.rs):

1. `new_in` (4 errors, vec.rs:177): `InnerVec::from_raw_parts` needs `p.addr >= p.base`, `p.size >= 0`,
   `cap * size_of <= p.size`, and non-null/aligned for `cap > 0`, but `Allocator::alloc` (src/util/alloc.rs:125)
   has no spec, so the pointer it returns is unrefined. DEFERRED: needs an `Allocator::alloc` spec, decided by
   worker 1 (Box/Rc). With a spec of the shape
   `fn(&Self, layout: Layout[@s, @a]) -> Result<*mut u8{p: p.addr != 0 && p.addr % a == 0 && p.base <= p.addr && p.addr + s <= p.base + p.size}, AllocError>`
   the four obligations follow from `Layout::array`'s `[cap * T::size_of(), T::align_of()]` index (the `*mut u8 as *mut T`
   cast keeps the ptr index). The `Vec` invariant already follows from `Layout::array`'s `is_valid_array_layout` Ok-index.
2. `into_boxed_slice` (vec.rs:317) and `<Vec as IntoIterator>::into_iter` (vec.rs:376): `core::ptr::read(&self.alloc)`.
   The `&A -> *const A` coercion yields an unrefined pointer in this flux revision, so flux-core's `read` precondition
   `valid(p, A::size_of())` is unprovable. Tried: no spec can help (the pointer comes from a coercion, not a call);
   `ptr::from_ref` has no flux-core spec either. Would need flux to model ref-to-raw-pointer coercions.
3. `<Vec as Clone>::clone` (vec.rs:83): `self.try_clone().expect(..)` requires `Result[true]` (flux-core `expect`
   spec) but allocation can genuinely fail. Tried: `try_clone` now has a precise spec (len/cap preserved) but its
   Ok-ness cannot be promised. Only a local rewrite to `match .. { Err(_) => panic!() }` would verify; not applied
   (annotations only).
4. `from_exact_iter` (vec.rs:33) and `from_array` (vec.rs:97): `push` needs `len < cap` inside a `for` loop over an
   `impl ExactSizeIterator` / `[T; N]`. flux-core has no spec for `ExactSizeIterator::len` nor for
   `core::array::IntoIter::next`, and even with one the loop invariant `o.len + size(iter) == cap` involves the
   uninterpreted `default_iterator_size(iter)`, which kvar inference cannot express. Tried: no spec on this function
   helps (strategy i); the struct refinement is not the issue (ii); an intermediate extern spec for
   `ExactSizeIterator::len` (iii) would still leave the loop invariant unprovable.

`IntoIter` (uid 2) was not re-attempted (its `into_iter` constructor is itself blocked by item 2).

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

## Wave 1, worker 5 (uids 94, 23, 52, 88, 43, 18, 33, 82)

### type 94 reader::Reader (src/reader.rs:37) — ABORT (3 errors remain, all in `fill_buffer`)

Errors (unchanged from baseline):
- src/reader.rs `fill_buffer`: `chunk.len() - self.chunk_used` may underflow; `chunk[self.chunk_used + i]`
  and `chunk[i]` possible out-of-bounds.

Root cause: `Chunk` (uid 67, src/stream.rs) is unrefined and its `Deref<Target = [u8]>` impl has no spec,
so every `chunk.len()` / `chunk[i]` goes through a separate `deref` call returning a `&[u8]` of an
unrelated existential length. Nothing on `Reader` can relate `chunk_used` to a length flux never sees.
Strategies: (i) a `fill_buffer` precondition cannot mention the chunk length; (ii) indexing `chunk_used`
in `Reader`'s refinement is useless without `Chunk`'s length; (iii) the missing callee spec is
`impl Deref for Chunk { fn deref(&Chunk[@c]) -> &[u8][c.len] }` plus `Chunk: refined_by(len)` (which in
turn needs `InnerVec`'s `Deref` to expose `v.len`), both outside this worker's files.
Deferred: needs Chunk (uid 67) refined by `len` + `Deref` spec, and `InnerVec::deref -> &[T][v.len]`.

Annotation kept (verified): `Reader` is `refined_by(offset: int)` (`full_offset`), `new -> Self[0]`,
`offset() -> usize[v.offset]`, and every `read_*`/`skip`/`expect_u8`/`strip_bytes`/`with_limit`/
`read_vec*` method is `&strg` with `ensures self: Self{w: w.offset >= v.offset}` (monotonic offset);
`peek_u8`/`fill_buffer` ensure `Self[v.offset]`. With these, `wasm.offset() - start` in
`CustomSection::read` (module.rs:498), `Module::read` (module.rs:180) and `read_function_code`
(code.rs:166) already verify in the full sweep.
Two caveats:
- `read_i64` is left with `ensures self: Self` (unconstrained). The monotonic ensures verifies but takes
  ~43s with cvc5 and ~60s with z3 (each of the ten `self.read_u8()?` creates a kvar over the whole scope;
  `read_i32` with five is ~5s). Strengthen it only if a caller needs it.
- Refined closure bounds (`where F: FnOnce(&mut Self{..})`) are rejected by this flux revision
  ("cannot determine corresponding unrefined predicate", for FnOnce and FnMut, refined or not, because
  the Rust bound is higher-ranked over the `&mut` lifetime). The `with_limit`/`read_vec*` specs therefore
  carry no closure bound; the monotonic `ensures` still verifies because flux infers the closure's
  `&mut Reader` parameter refinement at the call `f(self)`.

### type 23 ir_reader::IrReader (src/ir_reader.rs:22) — ABORT (1 of 2 errors remain)

Fixed: `offset < 256` for `.0.get_unchecked(offset)` via `JumpTarget::offset() -> usize{v: v < 256}`
(src/text.rs), which is proved from the `BitAndNonNegBound` primop property added to
src/util/flux_specs.rs (flux leaves `&` on ints uninterpreted).
Remaining: `code.get_unchecked(page)` needs `page < code.len()`. This is a trust assumption on the
compiler's IR (the interpreter reads addresses it does not validate; the `strict-assertions` feature
makes it a dynamic check). Strategies: (i) a `requires` on `IrReader::read` would just move the
obligation to `read_u32`/`read_u64`/`visit_instruction`, where after `address.advance(1)` the page can
change (offset 255 -> next page), so it is unprovable there and would add ~60 new errors; (ii) `JumpTarget`
has no invariant relating it to a particular `code` slice; (iii) `page()` cannot even be given
`-> usize[addr / 256]` since `>>` is uninterpreted. Deferred: unverifiable without a dynamic check.

### type 52 compiler::Compiler (src/compiler.rs:5) — ABORT (7 errors remain)

All seven errors are in `<Compiler as WasmVisitor>::call` (compiler.rs:211-245) indexing
`state.module().functions[..]`, `state.module().types[..]`, `state.store().modules()[module.0 as usize]`,
`state.store().host_modules()[..]`, `m.functions[..]`, `m.types[..]`, `hm.functions[..]`.
`Compiler` is a `PhantomData` struct with nothing to refine, and `call` is a trait method so it cannot take
preconditions. The facts needed are `Store` (uid 111, wave 4) and `Module`/`HostModule` invariants (every
`ModuleRef`/`HostModuleRef` and every `FuncIdx`/`TypeIdx` reachable from a validated module is in range).
Deferred: needs Store (uid 111) and Module (uid 110).

### type 88 code::Expr (src/code.rs:5) — ABORT (1 error remains)

`Expr::read_constant`: `value.unwrap()` after `wasm.read_code(&ConstantCompiler::new(store, module), &mut value)`.
`value: Option<Value>` is set to `Some` only inside the `ConstantCompiler` visitor's `finish`, reached through
`read_code`'s generic `WasmVisitor` dispatch. Strategies: (i) no precondition on `read_constant` can help
(`value` is local); (ii) `Expr` is a newtype with no state involved; (iii) the bridging spec would be on
`Reader::read_code` (`ensures state is Some on Ok`) which is only true for this one visitor and is not
expressible for the generic `S` state. Deferred: genuinely unverifiable without an `Option`-free API.

### type 43 module::Data (src/module.rs:944) and type 18 module::DataSection (src/module.rs:1003) — ABORT

Both prefixes report the same 3 errors in `DataSection::read` (module.rs:1015, 1023):
`store.modules()[i.0 as usize].memory` and `store.host_modules()[i.module.0 as usize].memory[i.index as usize]`.
`Data`/`DataSection` are unit structs; the indices come from `module.memory` (`MemoryKind::Import*`), whose
validity is a `Store`/`Module` invariant. Deferred: needs Store (uid 111).

### type 82 host::HostValList (src/host.rs:196) — ABORT (3 errors remain, 1 fixed)

Annotation kept (verified): `HostValList: refined_by(len: int)`, `invariant(len <= MAX_HOST_FUNCTION_PARAMS)`,
`as_slice -> &[ValType][v.len]`, `len -> usize[v.len]`, `is_empty -> bool[v.len == 0]`. Fixes `as_slice`.
Remaining:
- `HostValList::new`: `try_new(s).expect(..)` — `Ok`-ness depends on the characters of `s`; not expressible
  as a precondition (`&str` contents are not refined). Real panic by design (the const-literal constructor).
- `<HostValListIter as DoubleEndedIterator>::next_back` (host.rs `self.data.data[self.back]`) and the
  `Iterator::next` ICE (infer.rs:887, `&strg` vs `&mut`): these belong to `HostValListIter` (uid 98, wave 2,
  dep 82) which needs its own `refined_by(index, back)` + `invariant(index <= back <= len)` and the
  `assoc(size/done/step)` + `&strg` `next` spec. Not touched here to stay inside this worker's uids.

## Rc::new_uninit_slice_in — trusted (user-approved, solver divergence)

After wave 1, `Rc::new_uninit_slice_in` makes both z3 and cvc5 diverge. Commit-level bisect: terminates (10s,
8 unprovable errors) at 27bf062; diverges from e714bb1 (worker 1 merge). File-level forward bisect: applying
any one of worker 1's `rc.rs`, `alloc.rs`, or `flux_specs.rs` alone on the pre-worker-1 tree triggers it, so no
single spec is at fault; the body (Layout::array pow2 bitvector reasoning + `len * T::size_of()` + `extend`/
`pad_to_align` existentials) is at the solver's edge and any extra pointer fact tips it. Marked
`#[flux_rs::trusted(reason = "solver divergence")]` with the user's approval; its spec (pointer validity for
`len` values) is therefore an assumption. Trusted count 11 -> 12.

## type 102 memory::Memory (src/memory.rs:59) — DONE (wave 2, worker 2): 54 -> 0 errors, type check SAFE

Not an abort; recorded here because the refinement is cross-cutting (Store uid 111, Engine uid 114 and
the interpreter build on it).

Refinement: `Memory` is `refined_by(base: int, addr: int, bytes: int, size: int, has_alloc: bool, page: int)`
with fields `ptr: *mut[base, addr, bytes] u8`, `size: usize[size]`, `ty: MemType[page]`,
`allocator: Option<_>[has_alloc]` and invariants
`size <= bytes && size + 15 <= isize::MAX`, `addr >= base && bytes >= 0`, `addr == 0 || (addr > 0 && has_alloc)`,
`size == 0 || addr > 0`, `(page == 1 || page == 65536) && size % page == 0 && size / page <= u32::MAX`.

Public specs (all bounds checks are reflected in the `Result` index, no preconditions):
- `check_in_bounds(&Self[@m], addr, size) -> Result<(), MemoryError>[addr + size <= m.size]` (private)
- `store_u8/u16/u32/u64(&Self[@m], addr, i) -> Result<(), MemoryError>[addr + {1,2,4,8} <= m.size]`
- `load_u8/u16/u32/u64(&Self[@m], addr) -> Result<uN, MemoryError>[addr + {1,2,4,8} <= m.size]`
- `store(&Self[@m], addr, data: &[u8][@n]) -> Result<(), MemoryError>[addr + n <= m.size]`
- `load(&Self[@m], addr, len) -> Result<&[u8][len], MemoryError>[addr + len <= m.size]`
- `effective_address(base: u32, offset: u32) -> Result<usize[base + offset], MemoryError>` (Ok-ness left
  existential: on 64-bit it is always Ok, but flux does not carry the `u32` upper bounds by default)
- `zero() -> Memory[0, 0, 0, 0, false, 65536]`; `new(ty: MemType[@t], allocator: _) -> Result<Memory{m: m.has_alloc && m.page == t.page}, AllocError>`
- `grow(&mut Self, n: u32) -> Result<u32, MemoryError>` (default spec; `&mut` keeps the invariant, so callers
  going through `Rc::get_mut` need no strong reference)
- `size(&Self[@m]) -> u32[m.size / m.page]`, `mem_type(&Self[@m]) -> MemType[m.page]`,
  `is_zero(&Self[@m]) -> bool[m.addr == 0 && !m.has_alloc]`, `get_slice(&Self[@m]) -> &[u8][m.size]`,
  `as_shared_cells(&Self[@m]) -> &[UnsafeCell<u8>][m.size]`
- trait `WasmMemoryAllocator::allocate/reallocate(.., layout: Layout[@l]) -> Result<NonNull<u8>{p: p.addr > 0 &&
  p.base == p.addr && p.size == l.size && p.addr % l.align == 0}, AllocError>` (same contract as
  `Allocator::alloc`; no in-crate non-test impls, so nothing to restate).

Cross-cutting additions in src/util/flux_specs.rs (append-only):
- Detached refinements of `types::MemPageSize` (`refined_by(size: int)`, variants `_1 -> [1]`, `_65536 -> [65536]`,
  invariant `size == 1 || size == 65536`) and `types::MemType` (`refined_by(page: int)`, field
  `page_size: MemPageSize[page]`, invariant `page == 1 || page == 65536`) plus specs `MemPageSize::size -> usize[p]`,
  `MemPageSize::alignment -> usize{(p == 1 && v == 1) || (p == 65536 && v == 16)}`, `MemType::zero -> MemType[65536]`,
  `MemType::page_size -> usize[t.page]`, `MemType::page_alignment` (same as alignment). types.rs itself is untouched;
  all of types.rs still verifies (`read`, `zero`, `can_hold`, `matches`, `min` re-checked) and the full sweep shows
  no new errors in module.rs / imports.rs. `MemType` values elsewhere are now `MemType{page}` existentials.
- `extern_spec impl From<u32> for u64 { fn from(u32[@x]) -> u64[x] }` (flux-core only covers `TryFrom`).

Flux details worth knowing:
- `dyn Trait` is not accepted in spec syntax; `_` works as a hole for `Option<Rc<dyn ..>>` fields / params.
- `grow` needs `#[flux_rs::opts(solver = "z3", check_overflow = "lazy")]` (cvc5 times out; `lazy` is what gives
  the `u32` range of page counts, needed for `size + 15 <= isize::MAX`); `new` uses `check_overflow = "lazy"` too.
- One semantics-preserving local rewrite in `Drop::drop`: `self.allocator.take().unwrap()` ->
  `self.allocator.as_ref().unwrap()`. `take()` left the struct with a non-null `ptr` and `allocator == None` at the
  fold point, violating `addr == 0 || has_alloc`; the `Rc` is dropped by the field drop glue right after the body
  either way (same order: after `deallocate`).

### type 82 host::HostValList (src/host.rs:196) — update (wave 2): 1 error remains (documented panic)

The two `HostValListIter` errors listed above are fixed by uid 98 (`refined_by(index, back, len)`,
`&strg` `next`/`next_back`, `HostValList::iter -> HostValListIter[0, v.len, v.len]`). The only remaining
error is `HostValList::new`: `try_new(s).expect(..)`. `try_new` is `Err` exactly when `s` contains a
character outside `iIfd` or more than `MAX_HOST_FUNCTION_PARAMS` characters; `&str` contents are not
refined, so no precondition can express it. This is the documented const-literal constructor panic
(see its doc comment); `try_new` is the checked API. Strategies: (i) a `requires` on `new` — nothing to
state about `s`; (ii) nothing on the struct helps (the struct is built after the check); (iii) a spec on
`try_new` cannot make it `Result[true]` for arbitrary `s`. Kept as a documented panic.

### type 99 host::HostFunction (src/host.rs:329) — ABORT (1 error, documented panic)

`HostFunction::new`: `try_new(..).expect(..)`. `try_new` (already `trusted` from wave 1, infer.rs:1022
ICE on the `Box<dyn Fn>` unsize cast) returns `Err` on (a) `returns.len() > 1`, (b) the
`Box::new(f)?` allocation failure. (a) could be stated as `requires returns.len <= 1`, but (b) is an
`AllocError` from `Box::new`, which has no `Result[true]` spec and genuinely can fail, so `expect` is
unprovable on any precondition. `new`'s doc comment already lists both as panics and points callers to
`try_new`. Strategies: (i) `requires` on `new` — cannot rule out allocation failure; (ii) `HostFunction`
has no state that helps; (iii) a `-> Result<_, _>[true]` spec on `try_new` would be a false spec (and
`try_new` is trusted, so it would be unchecked). Kept as a documented panic.

### type 110 module::Module (src/module.rs:28) — ABORT (7 -> 6 errors; 3 blocked on Box<[T]>, 3 on Store)

Kept (verified, no new errors):
- `Module: refined_by(has_table: bool, has_memory: bool)` (`table`/`memory` are `Option[..]`-indexed) and
  `check_memory_defined -> Result<(), _>[m.has_memory]`. This also removed the "error jumping to join point"
  in `get_table`.
- Index newtypes (`TypeIdx`, `FuncIdx`, `TableIdx`, `MemIdx`, `GlobalIdx`, `LocalIdx`, `LabelIdx`):
  `refined_by(idx: int)`, `.0: u32[idx]`, `From<u32>: fn(v: u32[@n]) -> Self[n]`.
- `#[flux_rs::opts(solver = "z3")]` on `Module::read_function_code` (code.rs): with the index-type
  refinement the body takes ~65s under cvc5 (harness TIMEOUT) and ~10s under z3. Note that flux's query
  cache (`target/FLUXCACHE`) makes re-runs of an unchanged constraint look instant, so time it after a change.

Remaining errors:
- code.rs `read_function_code` (3x `self.functions[i]`): the section fields are `Box<[T]>`, and the crate's
  `util::box_::Box` is `refined_by(base, addr, size)` (allocation only) with `Deref: fn(&Self[@v]) -> &T`
  (unrefined pointee), so every `self.functions.len()` / `self.functions[i]` is a fresh existential length.
  A length-exposing `Module` refinement (`refined_by(n_funcs, ..)` with `#[field(Box<[Func]>{..})]`) is not
  expressible: `Box<[T]>` has no length component and the length lives in fat-pointer metadata flux does not
  model, so even `box_.rs` could only provide it through a trusted bridge (e.g. a trusted
  `Box<[T]>::len -> usize[v.len]` plus a `len` refinement param set by `into_boxed_slice`). Outside this
  worker's files; see the cross-cutting note in the handback.
  Strategies tried: (i) a `requires i < ..` on `read_function_code` has nothing to refer to; (ii) refining
  `Func` by `(stack_usage, local_size[, parameter_size])` to at least give `required_stack_words` an exact
  value adds 2 "assignment might be unsafe" errors in `read_function_code` (`f.local_size = ..`,
  `(f.expr, f.stack_usage) = ..` through the `&mut f` captured by the `with_limit` closure) and tips the
  function over the timeout, so `Func` is left unrefined; (iii) no intermediate callee is involved.
- `get_table` (3x `store.modules_mut()[i.0]`, `store.host_modules_mut()[..].table[..]`): the `ModuleRef` /
  `HostRef` stored in `TableKind::Import*` are validated at import time against the `Store`; needs the
  `Store` refinement (uid 111). Deferred.
- Not counted under this uid but in the same prefix run: `DataSection::read` (3, Store) and
  `ExportSection::read` (`out.push(e)` needs `len < cap`; `Vec::new(len)` gives `cap == len` but the loop
  index `0..len` is not related to `out.inner.len` without a loop invariant flux cannot infer here). Deferred.

## Wave 2, worker 1: type 114 engine::Engine (src/engine.rs:4) and type 21 interpreter::Interpreter (src/interpreter.rs)

### type 114 engine::Engine — ABORT (its own constructor verifies; `call_impl`/`invoke`/`resume` keep 21 errors)

Refinement kept (src/engine.rs): `refined_by(base: int, addr: int, bytes: int, size: int, sp: int, fp: int)`,
`invariant(fp <= sp && sp <= size)`, fields `fp: u32[fp]`, `sp: usize[sp]`, `stack: Stack[base, addr, bytes, size]`.
`Engine::new`/`reset` (src/store.rs) verify (with the detached `Stack::new -> Result<Stack{s: s.size == size}, _>`
spec appended to src/util/flux_specs.rs: `Engine::new` needs `0 <= size`). `clear_memory`/`clear_table` got detached
specs `fn(self: &mut Engine[@e])` there too (they only touch unrefined fields; `memory_grow` needs the indices preserved).
Inherent methods in src/interpreter.rs carry `fn(self: &strg Engine[@e], ..) -> Result<..> ensures self: Engine`
(`call_impl`, `call_impl_enter_module`, `invoke`, `resume`); `call_impl_enter_module` verifies.
Remaining 21 errors, all deferred to other waves/files:
- 11 stack-headroom obligations (`write_u32(self.sp..)`, `self.sp += 2 + f.local_size` fold, `invoke`'s `expect`):
  the guard `self.stack.len() < self.sp + f.required_stack_words()` is there, but `Function::required_stack_words`
  (src/module.rs) is unspecified; it needs `fn(&Self[@f]) -> usize{v: v >= 2 + f.local_size}` with `Function` refined by
  `local_size` (Module wave). `invoke` additionally needs `params_words` related to the loop (sum over `ty.params`).
- 5 in `resume`: the host-pause result push `write_*(self.sp)` needs `sp + 2 <= size`; that headroom was reserved by
  the validator for the paused `call_host`, nothing in the engine records it (an honest `requires e.sp + 2 <= e.size`
  would just move the error to every test caller).
- 5 out-of-bounds indexes `store.modules()[..]`, `m.functions[..]`, `m.types[..]` (Store/Module wave).
Strategies: (i) specs on this function — the missing facts are callee outputs; (ii) stronger struct refinement — a
`required <= size - sp` field does not exist in `Engine`; (iii) callee specs — the callees are in module.rs/store.rs,
outside this worker's files.

### type 21 interpreter::Interpreter — ABORT (856 -> 274 errors: 198 "validator assumption" + 76 residual)

Final shape (src/interpreter.rs, src/visitor.rs): the visitor traits declare associated refinements with `true` defaults
(`BaseVisitor`: `has_depth(s: Self::State, n: int)`, `has_room(s: Self::State, pops: int, pushes: int)`; `IrVisitor`:
`has_local(s, l: LocalVariable)`, `can_branch(s, l: LabelTarget)`, `can_branch_after_pop(s, l)`); the interpreter defines
them (`has_depth = s.sp >= s.fp + n`, `has_room = s.sp - pops + pushes <= s.size`, `has_local`, `can_branch` =
`sp >= fp + depth(l) && depth(l) >= arity(l) && sp >= fp + arity(l) + 2`). Every instruction body lives in an inherent
`Interpreter::<name>_op` method whose spec is
`fn(&Self, <params>, state: &strg Engine[@s]) -> Result<(), InterpreterBreak> requires <Interpreter as BaseVisitor>::has_depth(s, P) [&& has_room(s, P, Q)] ensures state: Engine`
(P/Q = words popped/pushed; the `instruction!` macro arms carry the spec per shape), and the trait impls are
`forward!` wrappers with the spec `fn(&Self, <params>, state: &strg Self::State[@s]) -> Result<(), Self::Error> ensures state: Self::State`
(no `requires`). This is a semantics-preserving rewrite (pure delegation; the wrappers inline) and is the only shape
under which flux verifies the bodies — see "why" below. Pops are over-approximated by at most 2 words where the
operand type is not statically known (`drop`, `select`, `local_set/tee`, `global_set*`): a frame always holds its
2-word header below the operand stack, so `sp >= fp + 2` holds inside any frame and these preconditions are implied by
the validator's guarantee; pushes are stated exactly for the 1-word case (see residuals).
`LabelTarget`/`LabelArity` (src/text.rs, uids 55/56): `refined_by(bits: int)` / `refined_by(n: int)` with
`depth() -> u8[(l.bits / 4194304) % 256]`, `arity() -> LabelArity[(l.bits / 1073741824) % 4]`; the two bodies were
rewritten from `(x >> k) & m` to `(x / 2^k) % (m+1)` (identical on `u32`; flux does not reason about shifts/masks
on integers). `LocalVariable` (src/visitor.rs) is `refined_by(off: int)`; `LocalVariable::addr` has the spec
`fn(&Self[@l], fp: u32{fp <= i32::MAX && 0 <= fp + l.off && fp + l.off <= i32::MAX}) -> usize[fp + l.off]`.

Why not the trait-level route from wave 1 (measured, all reverted):
1. Trait-method specs on `BaseVisitor` (`fn(&Self, state: &strg Self::State[@s]) -> .. requires <Self as BaseVisitor>::has_depth(s, P) ..`)
   + assoc definitions on the interpreter impl + restated impl specs: interpreter.rs 856 -> 90 errors (bodies verified).
   But (a) flux PANICS (rustc `binder.rs:1285` "cannot find BoundConst in param-env", from
   `flux_refineck::checker::trait_impl_subtyping -> SortNormalizer -> normalize_projection_ty_with_rustc`) on the
   const-generic `impl BaseVisitor for Compiler<'a, const N, const M>` (src/compiler.rs), with or without specs restated
   on that impl — `cargo flux` on the crate aborts, so this cannot be merged; (b) every spec-less impl hits
   `internal flux error: incompatible types &strg State[s] - &mut State` (constant.rs: 0 -> 175) unless the `&strg`
   spec is restated on all its ~170 methods; (c) the generic dispatchers `IrReader::visit_instruction` (src/ir_reader.rs,
   1 -> 169) and `Expr::visit` (src/code.rs, 4 -> 179) cannot prove `<V as BaseVisitor>::has_depth(..)` for an abstract
   `V`, so the obligations simply move there.
2. Impl-only specs (no trait-method specs): flux reports `a precondition cannot be proved` at the `fn` line of each impl
   method (the impl requires more than the trait) AND SKIPS THE BODY — e.g. `i32_div_u`'s division-by-zero and
   `return_`'s invariant folds disappear. Verified by `--only-check` on single methods. So this shape verifies nothing.
3. Hence the `*_op` split: bodies verified under their preconditions; each `forward!` wrapper reports the precondition
   of its single `*_op` call as unprovable (198 errors = one per conjunct; e.g. `local_get` has 4). These are the exact
   points where the Wasm validator's stack-typing guarantee is assumed; nothing local can discharge them.

Residual 76 errors in the `*_op` bodies and `call_host`, by cause:
- 24 out-of-bounds indexes into `store.modules()[..]`, `m.functions[..]`, `m.types[..]`, `m.globals[..]`,
  `store.host_modules()[..]` (`call_impl` 2, `invoke` 3, `call_indirect_op` 7, `global_get_op` 2, `global_get_host_op`
  2, `global_set_op` 2, `global_set_host_op` 2, `call_host` 4): need `Store`/`Module`/`HostModule` refinements
  (ModuleRef/FuncIdx/TypeIdx/GlobalIdx vs vector lengths) — deferred to the Store/Module wave.
- 11 in `call_impl`/`invoke` and 5 in `resume`: see type 114 above.
- 15 in `call_host` (trait impl, no precondition possible): `state.sp -= f.param_size()` and the parameter reads/result
  pushes depend on the host function's signature looked up in the store (`f.param_size()`, `f.returns()`), and
  `sv.push(..).unwrap()` needs `f.params().len() <= MAX_HOST_FUNCTION_PARAMS`; also `state.call_host_fn(..)` is a
  `trusted` `&mut self` method with no spec, so the indices are lost after it. Needs `HostFunction` refinements
  (param_size/params len/returns) and a `call_host_fn(self: &mut Engine[@e], ..)` spec in store.rs.
- 9 in the `i64`/`f64` arms of `local_get_op` (2), `local_set_op`, `local_tee_op`, `global_get_op` (2),
  `global_get_host_op` (3): the word count of the operand depends on a `ValType` value (`l.ty`, `g.type_.ty`, the
  `Value` variant). `ValType` (src/types.rs) is unrefined; with `#[flux_rs::reflect]` on it the preconditions could be
  `has_room(s, 0, words(ty))`/`has_local(s, l, words(l.ty))` exactly. Stated conservatively for 1 word; the 2-word arms
  fail (`write_u64(state.sp, ..)` needs `sp + 2 <= size`).
- 4 in `return_op`: `return_fp = fp - frame_length`, `parameter_start = fp - parameter_size` (underflow) and the two
  final folds (`fp' <= sp'`): `frame_length`/`parameter_size` are read back from the stack word at `fp` (`CallFrame::from_bits`),
  i.e. the relation `frame_length <= fp`, `parameter_size <= fp` lives in stack *contents*, which the `Stack`
  refinement does not model.
- 6 in `br_table_op`: the label is `cases(v)` (closure result, data dependent), so `br_impl`'s `can_branch` cannot be
  stated; the spec only requires `has_depth(s, 1)`.
- 2 casts: `LocalVariable::addr` (`(fp as i32 + off as i32) as usize`: flux treats `u32 as i32` as unconstrained even
  under `fp <= i32::MAX`, so the postcondition `usize[fp + l.off]` is unprovable; the spec is kept because the three
  `local_*_op` callers verify against it) and `i32_div_u_op` (`(a as u32) / (b as u32)` after `if b == 0 { return .. }`
  on the `i32`: the `i32 -> u32` cast loses `b != 0`).
Verified clean: all 110 `instruction!`-generated ops, all loads/stores/consts/conversions, `memory_size/grow`,
`drop`, `select`, `if_`, `br`, `br_if`, `br_impl`, `call`, `call_extern`, `call_impl_enter_module`, `global_set*`
bodies apart from the index lookups, `LabelTarget::depth/arity`, `Engine::new/reset`.
Strategies: (i) per-function specs — done wherever a local precondition exists (see above); (ii) struct refinement —
`Engine` is refined as planned, the missing structure is `Store`/`Module`/`HostFunction`/`ValType`; (iii) callee specs —
added where the callee is in this worker's files (`Engine::clear_*`, `Stack::new` detached, `LocalVariable::addr`,
`LabelTarget::depth/arity`, `br_impl`, `return_op`); the rest are in store.rs/module.rs/types.rs.
Cross-check after the final state: compiler:: 7 (unchanged), constant:: 0 (unchanged), ir_reader:: 1 (unchanged),
code:: 4 (unchanged), text:: 22 (unchanged), store:: 17 (unchanged; `Engine::new` verifies). Full crate: 1032 -> 450 errors, no panics.
Scripts (scratchpad, worker 1): `rewrite_visitor.py`/`table.py` (stack-effect table), `add_impl_specs.py` (impl specs
from the table), `split_ops.py` (the `*_op`/`forward!` split), `classify.py` (error triage).

## Wave 2, worker 3 (uids 92, 67, 94, 71)

### type 92 text::CodeBuilder (src/text.rs:271) — ABORT (2 of 4 errors remain, both blocked on `Vec::deref_mut`)

Refinement kept (verified): `refined_by(pages: InnerVec, offset: int)` with `#[field(Vec<Box<TextPage>>[pages])]`,
`#[field(usize[offset])]`, invariant `offset <= 256 && (pages.len > 0 || offset == 0) && pages.cap < 16777216`
(= `MAX_ADDRESSABLE_CODE_PAGES`, 2^24). Specs: `new -> Result<CodeBuilder{v: v.pages.len == 0 && v.offset == 0}, _>`,
`pages() -> &[Box<TextPage>][v.pages.len]`, `offset() -> usize[v.offset]`, `add_page`/`push` are `&strg` and preserve
`pages.cap` (`push` ensures `w.pages.len >= v.pages.len`). This discharges the underflow `pages.len() - 1` and the
`.0[self.offset]` page-array index in `push` (4 -> 2 errors; `pc`, `write_32`, `read`, `read_32`, `backpatch` verify).

Semantics-preserving local rewrite (last resort, noted): `add_page` now takes the page it appends
(`fn add_page(&mut self, page: Box<TextPage>)`) and `push` calls `self.add_page(Box::new(TextPage::default())?)?`.
Reason: flux cannot couple a function's `Result` index with its `ensures` post-state, so with the `Box::new` inside
`add_page` its `Ok`-ness was undetermined and `push` could not know, after `self.add_page()?`, that a page exists and
`offset == 0`. With the allocation hoisted (same evaluation order and effects: the box is allocated, then `try_push`,
`?` on each), `add_page` is `Result<(), AllocError>[v.pages.len < v.pages.cap]` with an exact `ensures`.
Tried first (annotations only): (i) `ensures` disjunctions (`w == v || (w.offset == 0 && w.pages.len == v.pages.len + 1)`)
on `add_page` — `push` cannot exclude the unchanged disjunct after `?`; (ii) stronger invariant — the gap is between
`Ok` and the post-state, not in the struct; (iii) a callee spec — `Box::new` is genuinely fallible, so `add_page`'s
`Result` index cannot be determined without the rewrite.

Remaining 2 errors: `self.pages[idx]` in `push` (text.rs `.0[self.offset] = c` line) and `self.pages[page_index]` in
`write`: both index through `<Vec as DerefMut>::deref_mut`, whose spec in src/util/vec.rs is
`fn(self: &mut Self[@v]) -> &mut [T]` (no length), so `IndexMut` sees an existential slice length.
Confirmed fix (tested, then reverted because src/util/vec.rs is outside this worker's files): change it to
`fn(self: &mut Self[@v]) -> &mut [T][v.inner.len]` — uid 92 becomes RESULT=SAFE and the only new error is the body of
`Vec::deref_mut` itself (`self.inner.deref_mut()`, vec.rs:348), because `<InnerVec as DerefMut>::deref_mut`
(src/util/inner_vec.rs:138) is also `-> &mut [T]`; that one needs `-> &mut [T][v.len]` plus an extern spec for
`core::slice::from_raw_parts_mut` mirroring the existing `from_raw_parts` one in src/util/flux_specs.rs. A detached
`flux_rs::specs { impl DerefMut for Vec .. }` in flux_specs.rs is rejected ("multiple specifications").
No new errors in `TextBuilder` (uid 113: 18 before and after, same sites).

### type 67 stream::Chunk (src/stream.rs:6) — DONE

`Chunk: refined_by(inner: InnerVec)` with `#[field(InnerVec<u8>[inner])]` (a `refined_by(len: int)` with
`#[field(InnerVec<u8>{v: v.len == len})]` is rejected: "parameter `len` cannot be determined"), specs
`From<InnerVec<u8>>::from(value: InnerVec<u8>[@v]) -> Chunk[v]`, `Deref::deref(&Self[@c]) -> &[u8][c.inner.len]`
(so `chunk.len()`, `chunk.is_empty()` and `chunk[i]` agree) and `return_(self: &strg Self, stream: _) ensures self: Self`
(`core::mem::replace` has no flux-core spec; the post-state is left unconstrained on purpose — a `&strg` extern spec
for `replace` would also apply to `code.rs`'s `replace(&mut self.functions[i], ..)` through a weak `&mut`, so it was
not added). RESULT=SAFE; `CircularBuffer` (uid 0) and `InnerVec` (uid 1) unaffected.

### type 94 reader::Reader (src/reader.rs:37) — wave 2: the 3 `fill_buffer` errors are fixed; 1 pre-existing error left

Refinement is now `refined_by(offset: int, chunk_used: int)` with `#[field(usize[chunk_used])] chunk_used` and
`#[field(Option<Chunk{c: chunk_used <= c.inner.len}>)] next` (the Reader invariant "the consumed prefix of the pending
chunk never exceeds its length"). `new -> Self[0, 0]`; `fill_buffer`/`peek_u8` ensure `Self{w: w.offset == v.offset}`
(`chunk_used` free); every other `read_*`/`skip`/`expect_u8`/`strip_bytes`/`with_limit`/`read_vec*` spec is unchanged
(`ensures self: Self{w: w.offset >= v.offset}` still names the field, `read_i64` stays unconstrained). `fill_buffer`
keeps `#[flux_rs::opts(solver = "z3")]` (~13s solver time; cvc5 still diverges).

Semantics-preserving local rewrite (last resort, noted): in `fill_buffer`,
`.map(|inner| inner.into())` -> `.map(Chunk::from)` (the blanket `impl<T, U: From<T>> Into<U> for T` resolves to
exactly `Chunk::from`). Bisection: with `Chunk` refined, the first half of `fill_buffer` (copy from the pending chunk)
verifies in <1s, the second half (fetch a new chunk) diverges with BOTH z3 and cvc5 (>5 min) through the `Into` blanket
impl: flux instantiates `InnerVec<T>`'s invariant with `T = {u8[b] | $k(b)}` (a kvar-refined `u8` coming from the
generic `into`), so the constraint contains `addr mod <{u8|$k} as Sized>::align_of()` and `size_of()` of a
kvar-dependent type, which neither solver handles. With `Chunk::from` the whole function verifies in ~20s (z3).
Tried before the rewrite: (i) spec — the divergence is independent of `fill_buffer`'s own spec; (ii) refinement —
same with and without the `chunk_used` refinement; (iii) callee spec — an extern spec for the blanket `Into::into`
cannot transfer a refinement between two different sorts.

Remaining 1 error (pre-existing since the wave-1 merge, not in this worker's list): `read_vec_in`
(src/reader.rs:559) `out.push(read_element(self)?)` needs `Vec::push`'s `len < cap`. The loop is `for _ in 0..len`
with `len: u32`; flux-core only defines `Step::step_forward`/`size` for `usize` and `i32`, so for `u32` the counter
is uninterpreted and no loop invariant relates `out.inner.len` to it. Fix (tested, needs a file outside this
worker's set): add `#![cfg_attr(flux, feature(step_trait))]` to src/lib.rs and, in src/util/flux_specs.rs,
`#[flux_rs::extern_spec(core::iter)] #[flux_rs::assoc(fn step_forward(start: int, count: int) -> int { start + count }
fn size(lo: int, hi: int) -> int { hi - lo })] impl core::iter::Step for u32 {}` (the spec alone fails with
E0658 "use of unstable library feature `step_trait`").
Note: `--check-type=94` through the harness only fits in the 30s budget with a warm flux cache (a cold whole-type run
is ~47s wall, dominated by `fill_buffer`); it is RESULT=ERROR with exactly this one error.

### type 71 util::string::String (src/util/string.rs:11) — DONE (no invariant; `try_from(&str)` fixed)

`String` wraps `Box<[u8], A>`, whose index is `(base, addr, size)` and carries no slice length (see type 26), so a
`refined_by(len)` cannot be determined from the field and no length refinement is possible without changing `Box`.
The `Vec::push` error in `TryFrom<&str>` (`Vec::new(value.len())` then one push per byte of `value.as_bytes()`) is fixed
by extern specs for `str::len(&str[@s]) -> usize[str_len(s)]` and `str::as_bytes(&str[@s]) -> &[u8][str_len(s)]` in
src/util/flux_specs.rs (flux refines `str` by its contents and provides the `str_len` theory function; flux-core has no
`str` specs). RESULT=SAFE. `HostName` (33) / `HostValList` (82), which also call `str::len`/`as_bytes`, are unaffected
(82 keeps the same 3 errors logged above).

## Wave 3, worker 1 (uids 111, 105, 18/43, 52, 110)

### type 111 store::Store (src/store.rs:6) — ABORT (15 -> 11 errors in store.rs; 17 in imports.rs unchanged)

Kept (verified, no new errors in this worker's files):
- `Store: refined_by(modules: InnerVec, host: InnerVec)` with `#[invariant(modules.cap <= 256)]`, fields
  `modules: Vec<Module>[modules]`, `host_modules: Vec<HostModule>[host]` (the `refined_by(n_modules: int, ..)` +
  `Vec<Module>{v: v.inner.len == n_modules}` form was not tried: a `refined_by` param must be a bare index).
- Detached `ModuleRef(u8[idx])` / `HostModuleRef(u8[idx])` (`refined_by(idx: int)`) in src/util/flux_specs.rs
  (visitor.rs untouched; tuple-struct detached specs take no trailing `;`). `HostModuleRef::new` is left
  unspecified (narrowing cast in visitor.rs).
- Specs: `from_host_modules -> Result<Store{s: s.modules.len == 0 && s.modules.cap == max_modules && s.host == h}, _>`,
  `modules -> &[Module][s.modules.len]`, `host_modules -> &[HostModule][s.host.len]`, `modules_mut`/`host_modules_mut`
  unindexed (`Vec::deref_mut` is unindexed in this tree), `pop_module`/`push_module` with `&strg` post-states,
  `push_module -> Result<ModuleRef{r: r.idx == s.modules.len}, _>[s.modules.len < s.modules.cap]`,
  `get_memory/get_table/get_memory_mut(.., module_ref: ModuleRef{r: r.idx < s.modules.len})`.
  `Engine::new` (store.rs) still verifies.
- Local rewrite in `push_module`: `let index = self.modules.len(); self.modules.try_push(module)?; Ok(ModuleRef(index as u8))`
  (same value as `len() - 1` after the push; flux cannot couple `try_push`'s `Ok` to its post-state, so the
  subtraction was an "arithmetic may underflow"). The `as u8` is exact because `len < cap <= 256`.

Remaining 11 errors in store.rs, all the same gap — call it the **store-wide reference validity** gap:
- `get_memory` 113/120, `get_table` 133/140 (x2), `get_memory_mut` 169/176 (x2): the `ModuleRef` in
  `MemoryKind::Import` / `TableKind::Import` and the `HostRef` in `*::ImportHost` live inside the `Module` being
  looked up; they were range-checked by `link_*` (imports.rs) at load time, i.e. the invariant is
  "for every `i`, every ref stored in `modules[i]` is `< i`" (resp. `< host.len` and `< host_modules[m].memory.len`).
  Flux has no quantification over `Vec` elements, and the obvious encoding `Vec<Module{m: m.mem_import < modules.len}>`
  is ruled out because `Vec<T>` is invariant in `T` (`InnerVec` holds `*mut T`), so `push_module` could not
  re-type the vector when `modules.len` grows. The host arms additionally need `host_modules[m].memory.len`, a
  per-element fact.
- `get_memory_mut` 161 (`&mut self.modules[module_ref.0 as usize]`): goes through `Vec::deref_mut`, which is unindexed
  here; it is discharged by the other wave-3 worker's `DerefMut -> &mut [T][v.inner.len]` once merged (the precondition
  `r.idx < s.modules.len` is already in place).
- `Engine::needs_start` 242 (`self.store.modules()[module_ref.0 as usize]`): needs `Engine` to expose its store,
  `#[field(Store[store])]`, and `requires module_ref.idx < e.store.modules.len` (engine wave).
imports.rs (17, unchanged): `link_*` only index through refs read out of a module (`Ref::Extern`/`Ref::Host`,
`*Kind::Import*`: the validity gap above) or through `Box<[T]>` sections (`module.functions/types/globals[..]`:
the `Box<[T]>` length bridge, see "Module") or `HostModule`'s vectors (per-element fact).
Strategies: (i) preconditions — added where the ref is a parameter; (ii) struct refinement — done for `Store`,
`ModuleRef`, `HostModuleRef`; a `Module`-side `refined_by(mem_import, ..)` would not help without the quantified
`Store` invariant; (iii) callee specs — `Vec`/`InnerVec` already expose `len`; nothing else is involved.

Callers now carrying an obligation (interpreter.rs, not this worker's file: 274 -> 275 errors, +5/-4):
`call_impl_enter_module` (71/72), `memory_grow_op` (885), `return_op` (1717/1718) call `get_memory`/`get_table`/
`get_memory_mut(state.module)` and need `state.module.idx < state.store.modules.len`. For the engine wave:
`Engine` fields `#[field(Store[store])]` + `#[field(ModuleRef[module])]` (both exist as `refined_by` params to add) and
`requires e.module < e.store.modules.len` on `call_impl*`/`invoke`/`resume` and the `*_op` methods that touch the
store; it cannot be an `Engine` invariant because `Engine::new` starts with `module: ModuleRef(0)` on an empty store.
(The -4 are the `required_stack_words` obligations at 104/105/110/120, see type 105 below.)

### type 105 code::Func (src/code.rs:60) — DONE (type check SAFE; `read_function_code` keeps its 3 Box<[T]> errors)

`Func: refined_by(stack_usage: int, local_size: int, parameter_size: int)` (fields `u16[..]`/`u16[..]`/`u8[..]`) and
`required_stack_words -> usize[f.stack_usage + 2 + f.local_size]` (exact). This discharges 4 of the interpreter's
stack-headroom obligations (`call_impl` 104/105/110/120) without any change there.
The closure problem noted under type 110 is solved by a semantics-preserving local rewrite of
`Module::read_function_code`: the part of the body that runs under `wasm.with_limit` is now the inherent helper
`Module::read_function_body(&self, wasm, builder, store, f: Func) -> Result<Func, _>` (pure code motion), and the
closure moves the `Func` through an `Option` slot (`let Some(f) = slot.take() else { unreachable!() }; ...;
slot = Some(f)`), so the refined fields are only ever assigned on an owned `Func`. Two shapes that do NOT work,
for the record: (a) the closure capturing `f` by value (`let mut f = f;`) or (b) the `Option` slot with the whole
body still inside the closure both make fixpoint allocate >23 GB on `read_function_code` (189 kvars) and get
OOM-killed (flux reports `failed to run fixpoint: EOF while parsing a value` / `Killed`); (c) the original `&mut f`
capture gives the 2 "assignment might be unsafe" errors. With the split, `read_function_code` + `read_function_body`
check in ~5 s (z3 kept on `read_function_code`). `slot.unwrap()` is not provable (the upvar's `Some`-ness is not
tracked into the closure), hence the `let .. else { unreachable!() }` shape already used by `Store::get_*`.

### type 110 module::Module — re-ABORT after Store (6 errors, unchanged count)

- code.rs 161/176/199 (`self.functions[i]`, x3): deferred: Box<[T]> length bridge.
- module.rs 830 (`store.modules_mut()[i.0]`): `Store::modules_mut` is unindexed (`Vec::deref_mut`), and the ref comes
  from `TableKind::Import` — store-wide ref-validity gap (type 111). 838 (x2, host table): same gap + per-element
  `HostModule.table.len`. Nothing Store-side can discharge these.

### type 43 module::Data / type 18 module::DataSection — re-ABORT after Store (3 errors, unchanged)

`DataSection::read` 1031 (`store.modules()[i.0]`, `i` from `MemoryKind::Import`) and 1039 (x2, `host_modules()[..]
.memory[..]`): store-wide ref-validity gap (type 111), not the Box<[T]> gap. `Data::read` itself is clean.

### type 52 compiler::Compiler — re-ABORT after Store (7 errors, unchanged)

`<Compiler as WasmVisitor>::call` 211/212/244/245 (`functions[..]`/`types[..]`): Box<[T]> length bridge;
227/243 (`host_modules()[module.0]`, `modules()[module.0]` with the ref from `Module::get_func_ref`): store-wide
ref-validity gap; 228 (`hm.functions[index]`): per-element `HostModule.functions.len`. `call` is a trait method,
so no precondition is possible either way.

## Wave 3, worker 2 (Vec/InnerVec/Box leftovers; uids 1, 66, 26, 2, 92, 94)

### DONE: `Vec::deref_mut` length index (uids 1, 66, 92)
`<InnerVec as DerefMut>::deref_mut` now returns `&mut [T][v.len]` and `<Vec as DerefMut>::deref_mut`
`&mut [T][v.inner.len]`, via a new extern spec for `core::slice::from_raw_parts_mut` in
src/util/flux_specs.rs (mirror of the existing `from_raw_parts` one). uid 1 SAFE; uid 92
(`text::CodeBuilder`) 2 -> 0 errors (SAFE) with no change to text.rs.

### DONE: `Step for u32` (uid 94)
`#![cfg_attr(flux, feature(step_trait))]` in src/lib.rs + an extern spec
`impl core::iter::Step for u32 {}` with `assoc step_forward/size` identical to flux-core's `usize`
impl. uid 94 (`reader::Reader`) 1 -> 0 errors (SAFE). Plain `cargo build -p spacewasm` still
compiles, but now emits one `unexpected_cfgs` warning for the `flux` cfg name: Cargo.toml's
`[lints.rust] unexpected_cfgs.check-cfg` lists only `cfg(kani)` and needs `'cfg(flux)'` added
(Cargo.toml was outside this worker's file set, so the warning is left for the merge).

### type 2 util::vec::IntoIter — now refined; `next`/`next_back`/`drop` verify, `size_hint` ABORT
Refinement: `refined_by(base, buf, size, cap, start, end)` where `start`/`end` are indexed by their
own addresses (`*const[base, start, size - (start - buf)] T`, expressions in the non-bare index
positions are accepted). Invariants: the `InnerVec` facts on `buf`/`cap`/`size`, the `Vec`
layout invariant (for `Drop`'s `Layout::array(cap).unwrap()`), `buf <= start <= end`,
`end - buf <= size`, `end - buf <= cap * T::size_of()`, `(end - start) % T::size_of() == 0`
(guarded by `cap == 0 || T::size_of() == 0 ||`), start/end aligned, and `cap > 0 || start == end`.
`Iterator` assoc: `size = if T::size_of() == 0 { 0 } else { (end - start) / T::size_of() }`,
`done = start >= end`, `step = t.end == s.end && s.start <= t.start`; `next`/`next_back` have
`&strg` specs stepping `start`/`end` by `T::size_of()`.
Solver: cvc5 diverges on the `%`/`/ T::size_of()` terms (a run of `into_iter` was killed after
10 min), so `next`, `next_back` and `<Vec as IntoIterator>::into_iter` carry
`#[flux_rs::opts(solver = "z3")]` (each ~3 s of solving). A first ZST invariant
`T::size_of() != 0 || start == end` made z3 diverge in `into_iter` when combined with the `%`
invariant; `end - buf <= cap * T::size_of()` expresses the same consequence and is fast.
Result for uid 2: 8 errors + 1 internal error -> 2 errors, both in `size_hint`:
`(self.end as usize - self.start as usize) / size_of::<T>()` — pointer-to-usize casts are
uninterpreted in this flux revision (underflow unprovable) and `size_of::<T>()` may be 0 for flux
(division by zero); `Iterator::size_hint` is a trait method, so no precondition can be added.
ABORT for `size_hint` (not re-attempted beyond confirming, per the wave-3 instructions).

### type 66 util::vec::Vec — 7 errors in 6 functions remain (was 9 in 6; `deref_mut` fixed)
1. `new_in` (2 errors, vec.rs:180) — still blocked, now by two missing axioms rather than a missing
   spec: `InnerVec::from_raw_parts` needs `p.size >= 0` and `p.addr > 0`, while `Allocator::alloc`
   (src/util/alloc.rs) guarantees only `p.size == l.size` and `p.addr != 0`; `l.size == cap *
   T::size_of()` has no sign because flux has no `T::size_of() >= 0` axiom, and pointer addresses
   have no `addr >= 0` axiom. Tried: (i) `requires T::size_of() >= 0` on `new_in` — would propagate
   to the generic callers `Vec::new`, `Reader::read_vec_in<T>` (reader.rs) and `Vec::try_clone` /
   `Box<[T]>::try_clone` (whose `Clone::clone` impls cannot take a precondition), i.e. new errors in
   other files; (ii) relaxing the `InnerVec` invariant to `addr != 0` — breaks `try_push`/`pop`
   (`ptr.add(len)` needs `addr + len*size_of != 0`, which only follows from `addr > 0`); (iii) a
   ghost `let _size: usize = layout.size();` (usize non-negativity) discharges `p.size >= 0`, but
   nothing can give `p.addr > 0` (flux-core has no `ptr.addr()` spec), so no rewrite helps either.
   Fix belongs in alloc.rs: strengthen the `Allocator::alloc` trait/impl specs to `p.addr > 0 &&
   p.size >= 0` (both physically true: addresses and layout sizes are `usize`), as src/memory.rs
   already does for its allocator wrapper; then `new_in` verifies as is.
2. `into_boxed_slice`, `<Vec as IntoIterator>::into_iter` (1 each): `ptr::read(&self.alloc)` needs
   `A::size_of() >= 0`; `into_iter` is a trait method (no precondition possible), `into_boxed_slice`
   is called from `From<Vec> for Box<[T]>` (trait method) so a precondition would move the error.
3. `<Vec as Clone>::clone`: `try_clone().expect(..)` — documented panic on allocation failure; left.
4. `from_exact_iter`, `from_array`: unchanged (no flux-core spec for `ExactSizeIterator::len` /
   `array::IntoIter`), not re-attempted.

### type 26 util::box_::Box — 4 errors + 2 internal (was 5 + 2)
`Box<[T], A>::try_clone` now verifies through the semantics-preserving local rewrite
`let src: &[T] = self;` (one `Deref` call, so `src.len()`/`src[i]`/`Vec::new_in(.., src.len())`
share one slice length); noted in the checkpoint. Remaining: `into_inner` (2, `?Sized` struct
cannot carry `T::size_of()` facts), the two `Clone::clone` `expect`s (documented panics), and the
two `checker.rs:1877` internal errors on `*mut T as *mut dyn Trait` in src/host.rs.
