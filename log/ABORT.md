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
