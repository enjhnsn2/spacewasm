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
