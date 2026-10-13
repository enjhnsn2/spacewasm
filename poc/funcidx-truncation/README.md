# PoC: `FuncIdx` truncation in `Module::get_func_ref`

`Module::get_func_ref` / `get_global_ref` (`src/module.rs`) return
`Some(Ref::Module(i as u16))` after checking only `i < functions.len()`. Nothing
bounds a module's function (or global) count to `u16::MAX`, so a `call` to a
module-local index above 65535 is resolved to index `i & 0xFFFF` and silently
mis-dispatched, even though the module passes validation. Per the WebAssembly
spec such a module must be rejected, not accepted and run with the wrong
function. The behaviour is a silent wrong-function call, not a crash: the
wrapped index stays in bounds, so there is no memory-safety violation.

Every other index in the crate is range-checked against `u16` before use
(`call_indirect` in compiler.rs, `get_local` in text.rs, `instr_imm_8_or_16` in
text.rs); function and global indices are the gap. Fix: add
`|| i > u16::MAX as usize` to the guard in both functions so an oversized index
becomes `FunctionIdxOutOfRange` / `GlobalIdxOutOfRange`.

## Reproduce from the command line

```sh
python3 generate.py --mode bug     funcidx_truncation.wasm
python3 generate.py --mode control funcidx_control.wasm

cargo run -q -p spacewasi -- funcidx_truncation.wasm   # prints: This is the wrong function target
cargo run -q -p spacewasi -- funcidx_control.wasm      # prints: This is the right function target
```

Both runs exit cleanly (status 0). The module imports
`wasi_snapshot_preview1.fd_write`, carries a linear memory with two strings, and
exports `_start`, which does `call <target>`, where the target function prints
its line and returns.

- **bug**: `_start` names local function 65536. It wraps to local function 0,
  which prints `This is the wrong function target`.
- **control**: identical module, but `_start` names local function 65534, below
  the wrap threshold. No truncation, so it prints `This is the right function
  target`.

The only difference between the two files is whether the called local index
crosses 65536.

## Reading the module (text form)

The module has 65537 functions, so the full text is machine-sized, but the
shape is small:

- `funcidx_truncation.annotated.wat` — a ~60-line **annotated, abbreviated**
  reading copy. The 65534 identical filler functions and the data bytes are
  elided. Not assemblable; it is there to be read.
- `generate.py --format wat <out.wat>` — the **full, assemblable** text (one
  line per function, ~65k lines). `spacewasi` only reads the binary, so this is
  for inspecting or assembling with `wat2wasm` / `wasm-tools` yourself.

The whole trick, in three lines of the module:

```wat
(import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))  ;; func index 0
(func $victim (type $void) i32.const 1 i32.const 76 i32.const 1 i32.const 0 call $fd_write drop)    ;; func index 1 = local 0
(func (export "_start") (type $void) call 65537)                                                    ;; names local 65536
```

`_start` names function index 65537, i.e. module-local index 65536.
`get_func_ref` casts that local index to `u16`, turning 65536 into 0, so the
call lands on `$victim` (local 0), which prints the wrong line.

## Files

- `generate.py` — builds the module (`--mode bug|control`, `--format wasm|wat`).
- `funcidx_truncation.wasm`, `funcidx_control.wasm` — prebuilt binaries for `spacewasi`.
- `funcidx_truncation.annotated.wat` — short reading copy.
- `funcidx_truncation.wat` — full assemblable text (bug mode).
