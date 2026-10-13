;; ANNOTATED / ABBREVIATED text form of funcidx_truncation.wasm.
;;
;; For reading only. NOT assemblable as-is: the 65534 filler functions are
;; elided (marked below), and the data bytes are summarized. The real module is
;;   python3 generate.py --mode bug            funcidx_truncation.wasm   (binary, run by spacewasi)
;;   python3 generate.py --mode bug --format wat  funcidx_truncation.wat  (full, assemblable text)
;;
;; WebAssembly's function index space puts imports first. The one imported
;; function (fd_write) is index 0, so the module's own functions are indices
;; 1, 2, 3, ... spacewasm's get_func_ref subtracts the import count (1) to get a
;; module-LOCAL index, then casts it to u16 before use. That cast is the bug.

(module
  ;; function index 0 : imported host function (writes an iovec to a file descriptor).
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))

  (type $w (func (param i32 i32 i32 i32) (result i32)))   ;; type 0 : fd_write
  (type $void (func))                                     ;; type 1 : every module function

  ;; Linear memory holds both strings and a WASI iovec {ptr,len} for each:
  ;;   offset  8 : "This is the wrong function target\n"
  ;;   offset 42 : "This is the right function target\n"
  ;;   offset 76 : iovec -> wrong string   (used by $victim)
  ;;   offset 84 : iovec -> right string   (used by $target)
  (memory 1)
  (export "memory" (memory 0))
  (data (i32.const 8) "..the two strings and their iovecs..")

  ;; function index 1  (local 0) -- the WRONG function the bug reaches.
  ;; _start's call names local 65536; 65536 & 0xFFFF == 0, i.e. local 0, i.e. this.
  (func $victim (type $void)
    i32.const 1      ;; fd = stdout
    i32.const 76     ;; iovec -> "This is the wrong function target"
    i32.const 1      ;; one iovec
    i32.const 0      ;; where to store bytes-written
    call $fd_write
    drop)

  ;; function index 2  (local 1) -- the exported entry point.
  (func (export "_start") (type $void)
    call 65537)      ;; names function index 65537 == local 65536 ($target).
                     ;; get_func_ref takes local index 65536, casts it to u16 -> 0,
                     ;; so this dispatches to $victim (local 0) instead.

  ;; function indices 3 .. 65536  (locals 2 .. 65535):
  ;;     (func (type $void))   x 65534        <-- 65534 empty no-ops, omitted here.
  ;; They exist only to push the module past 65536 functions so the cast wraps.

  ;; function index 65537  (local 65536) -- the INTENDED target of the call.
  ;; A correct engine runs this (prints the RIGHT line) or rejects the whole
  ;; module for having more functions than a u16 index can name.
  (func $target (type $void)
    i32.const 1      ;; fd = stdout
    i32.const 84     ;; iovec -> "This is the right function target"
    i32.const 1
    i32.const 0
    call $fd_write
    drop)
)
