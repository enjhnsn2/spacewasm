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
