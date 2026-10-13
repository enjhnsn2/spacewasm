#!/usr/bin/env python3
"""Generate a .wasm (or .wat) file that reproduces FuncIdx truncation in spacewasm.

`Module::get_func_ref` (src/module.rs) resolves a module-local function index as
`Some(Ref::Module(i as u16))` after only checking `i < functions.len()`. Nothing
bounds a module's function count to u16::MAX, so a `call` to a local index
`i > 65535` is resolved to function `i & 0xFFFF` and silently mis-dispatched,
even though the module passes validation. (`get_global_ref` is identical for
globals; the compiler's `write_16` truncates the IR immediate the same way.)

The generated module imports `wasi_snapshot_preview1.fd_write`, declares a linear
memory holding two strings, and exports `_start`, which does `call <TARGET>`:

    * the TARGET function prints "This is the right function target"
    * local function 0 prints "This is the wrong function target"

In `--mode bug` the called local index is 65536, which wraps to local function 0,
so running it through `spacewasi` prints the WRONG line. In `--mode control` the
target is local 65534 (below the wrap threshold) and it prints the RIGHT line.
The only difference between the two files is whether the called index crosses
65536. Both exit cleanly (status 0).

Usage:
    python3 generate.py --mode bug     funcidx_truncation.wasm
    python3 generate.py --mode control funcidx_control.wasm
    python3 generate.py --mode bug --format wat funcidx_truncation.wat
"""
import argparse
import struct

WRONG = b"This is the wrong function target\n"
RIGHT = b"This is the right function target\n"

# Memory layout (offset 0..3 is fd_write's nwritten scratch, left zero):
BASE = 8
OFF_W = BASE
OFF_R = OFF_W + len(WRONG)
IOV_BASE = (OFF_R + len(RIGHT) + 3) & ~3   # 4-byte aligned
OFF_IOV_W = IOV_BASE                        # iovec {ptr, len} for the wrong string
OFF_IOV_R = IOV_BASE + 8                    # iovec {ptr, len} for the right string

N_LOCAL = 65537     # local indices 0..=65536
VICTIM = 0          # == 65536 & 0xFFFF: the function wrongly run in --mode bug
START = 1           # exported as _start


def uleb(v):
    out = bytearray()
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            b |= 0x80
        out.append(b)
        if not v:
            break
    return bytes(out)


def sleb(v):
    """Signed LEB128 (i32.const operands may exceed 63, so a plain uleb is wrong)."""
    out = bytearray()
    while True:
        b = v & 0x7F
        v >>= 7
        if (v == 0 and not (b & 0x40)) or (v == -1 and (b & 0x40)):
            out.append(b)
            return bytes(out)
        out.append(b | 0x80)


def section(ident, body):
    return bytes([ident]) + uleb(len(body)) + body


def data_blob():
    blob = bytearray(OFF_IOV_R + 8 - BASE)
    def put(abs_off, data):
        blob[abs_off - BASE:abs_off - BASE + len(data)] = data
    put(OFF_W, WRONG)
    put(OFF_R, RIGHT)
    put(OFF_IOV_W, struct.pack("<ii", OFF_W, len(WRONG)))
    put(OFF_IOV_R, struct.pack("<ii", OFF_R, len(RIGHT)))
    return bytes(blob)


def print_body(iov_off):
    # fd_write(fd=1, iovs=iov_off, iovs_len=1, nwritten=0); drop errno
    return (bytes([0x00])                       # 0 local declarations
            + bytes([0x41]) + sleb(1)           # i32.const 1   (stdout)
            + bytes([0x41]) + sleb(iov_off)     # i32.const iov
            + bytes([0x41]) + sleb(1)           # i32.const 1   (iovs_len)
            + bytes([0x41]) + sleb(0)           # i32.const 0   (nwritten ptr)
            + bytes([0x10]) + uleb(0)           # call fd_write (import, func index 0)
            + bytes([0x1A])                     # drop
            + bytes([0x0B]))                    # end


def build(target_local):
    target_funcidx = 1 + target_local
    m = bytearray(b"\x00asm\x01\x00\x00\x00")
    # Types: 0 = (i32,i32,i32,i32)->(i32) for fd_write, 1 = ()->()
    m += section(1, uleb(2)
                 + bytes([0x60, 0x04, 0x7F, 0x7F, 0x7F, 0x7F, 0x01, 0x7F])
                 + bytes([0x60, 0x00, 0x00]))
    # Import: wasi_snapshot_preview1.fd_write : type 0  -> function index 0
    mod, nm = b"wasi_snapshot_preview1", b"fd_write"
    m += section(2, uleb(1) + uleb(len(mod)) + mod + uleb(len(nm)) + nm + bytes([0x00]) + uleb(0))
    # Functions: N_LOCAL locals, all type 1
    m += section(3, uleb(N_LOCAL) + bytes([0x01]) * N_LOCAL)
    # Memory: one memory, min 1 page, no max
    m += section(5, uleb(1) + bytes([0x00]) + uleb(1))
    # Exports: _start (func) and memory (mem)
    s = b"_start"
    mem = b"memory"
    m += section(7, uleb(2)
                 + uleb(len(s)) + s + bytes([0x00]) + uleb(1 + START)
                 + uleb(len(mem)) + mem + bytes([0x02]) + uleb(0))
    # Code: N_LOCAL bodies
    code = bytearray(uleb(N_LOCAL))
    for i in range(N_LOCAL):
        if i == VICTIM:
            body = print_body(OFF_IOV_W)
        elif i == target_local:
            body = print_body(OFF_IOV_R)
        elif i == START:
            body = bytes([0x00, 0x10]) + uleb(target_funcidx) + bytes([0x0B])  # call target; end
        else:
            body = bytes([0x00, 0x0B])  # empty no-op
        code += uleb(len(body)) + body
    m += section(10, bytes(code))
    # Data: initialize strings + iovecs at BASE
    blob = data_blob()
    seg = bytes([0x00]) + bytes([0x41]) + sleb(BASE) + bytes([0x0B]) + uleb(len(blob)) + blob
    m += section(11, uleb(1) + seg)
    return bytes(m), target_funcidx


def build_wat(target_local):
    target_funcidx = 1 + target_local

    def pr(iov, note):
        return (f"i32.const 1 i32.const {iov} i32.const 1 i32.const 0 "
                f"call 0 drop)  ;; {note}")

    out = [
        "(module",
        '  ;; function index 0: imported host function',
        '  (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))',
        "  (type (;0;) (func (param i32 i32 i32 i32) (result i32)))  ;; fd_write",
        "  (type (;1;) (func))                                       ;; every module function",
        "  (memory (;0;) 1)",
        '  (export "memory" (memory 0))',
        f'  (data (i32.const {BASE}) "..strings + iovecs, see generate.py..")',
        "  ;; module functions follow, at indices 1, 2, 3, ...",
    ]
    for i in range(N_LOCAL):
        fidx = 1 + i
        if i == VICTIM:
            out.append(f"  (func (;{fidx};) (type 1) {pr(OFF_IOV_W, 'local 0: prints WRONG')}")
        elif i == target_local:
            out.append(f"  (func (;{fidx};) (type 1) {pr(OFF_IOV_R, f'local {i}: prints RIGHT (target)')}")
        elif i == START:
            out.append(f'  (func (;{fidx};) (export "_start") (type 1) call {target_funcidx})  ;; local {i}: calls local {target_local}')
        else:
            out.append(f"  (func (;{fidx};) (type 1))")
    out.append(")")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("out")
    ap.add_argument("--mode", choices=["bug", "control"], default="bug")
    ap.add_argument("--format", choices=["wasm", "wat"], default="wasm",
                    help="wasm (binary, what spacewasi reads) or wat (full assemblable text)")
    args = ap.parse_args()
    target_local = 65536 if args.mode == "bug" else 65534
    wrap = target_local & 0xFFFF
    truncates = wrap != target_local
    line = "WRONG" if (args.mode == "bug" and truncates) else "RIGHT"

    if args.format == "wat":
        text = build_wat(target_local)
        with open(args.out, "w") as f:
            f.write(text)
        print(f"wrote {args.out}: {text.count(chr(10))} lines (assemble with wat2wasm / wasm-tools)")
    else:
        data, fidx = build(target_local)
        with open(args.out, "wb") as f:
            f.write(data)
        print(f"wrote {args.out}: {len(data)} bytes; _start does `call {fidx}` (local {target_local})")

    if args.mode == "bug":
        print(f"  local {target_local} > 65535 -> truncates to local {wrap}; spacewasi prints the {line} line")
    else:
        print(f"  local {target_local} < 65536 -> no truncation; spacewasi prints the {line} line")


if __name__ == "__main__":
    main()
