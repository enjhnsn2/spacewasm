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
