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
