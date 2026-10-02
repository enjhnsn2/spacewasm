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
