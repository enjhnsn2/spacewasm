//! Proof of concept: `FuncIdx` truncation in `Module::get_func_ref`.
//!
//! `Module::get_func_ref` resolves a module-local function index as
//! `Some(Ref::Module(i as u16))` after only checking `i < functions.len()`.
//! Nothing bounds a module's function count to `u16::MAX`, so a module with
//! more than 65536 functions makes the `as u16` cast wrap: a `call` to a local
//! index `i > 65535` resolves to function `i & 0xFFFF` and is silently
//! mis-dispatched, even though the module passes validation. (The compiler's
//! `write_16(index)` truncates the IR immediate the same way; `get_global_ref`
//! has the identical shape for globals.)
//!
//! Per the WebAssembly spec an engine that cannot represent such a module must
//! reject it, not accept it and run the wrong function.
//!
//! The module built here has 65537 functions where:
//!   * function 0      returns 11   (the *wrong* target: 65536 & 0xFFFF == 0)
//!   * function 65536  returns 22   (the *intended* target)
//!   * function 1      is exported as "entry" and does `call 65536`
//! A correct engine returns 22 (or rejects the module at load); an engine with
//! the truncation bug returns 11.
//!
//! Run with:
//!   cargo test -p spacewasm --test poc_funcidx_truncation -- --nocapture

use core::alloc::Layout;
use core::ptr::NonNull;

use spacewasm::{
    AllocError, Allocator, CodeBuilder, CompilerOptions, Engine, ExportDesc, InnerVec, Interpreter,
    InterpreterResult, InterpreterRunner, Module, ModuleRef, Ref, ValType, Value, WasmMemoryAllocator,
    WasmRef, WasmStream, global_allocator,
};

extern crate std;

const MAX_CONTROL_FRAMES: usize = 128;
const MAX_STACK_DEPTH: usize = 256;

// Module shape.
const N_FUNCS: u32 = 65537; // indices 0..=65536
const ENTRY: u32 = 1; // exported, reachable without truncation
const VICTIM: u32 = 0; // == TARGET & 0xFFFF: the wrong function actually run
const TARGET: u32 = 65536; // the function the source `call` names
const VICTIM_RET: i32 = 11;
const TARGET_RET: i32 = 22;

// `CodeBuilder::new` and the decoder allocate through the global allocator, so
// this test binary must provide the `__spacewasm_*` symbols. Back them with the
// system heap (same pattern as tests/validation_rejections_integration.rs).
struct SystemAllocator;

unsafe impl Allocator for SystemAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            Err(AllocError::AllocationFailed)
        } else {
            Ok(ptr)
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { std::alloc::dealloc(ptr, layout) }
    }
}

global_allocator!(SystemAllocator, SystemAllocator);

/// Single-shot stream that hands its whole buffer over once, then reports EOF.
struct ByteStream {
    buffer: std::vec::Vec<u8>,
    consumed: bool,
}

impl WasmStream for ByteStream {
    fn read(&mut self) -> Result<Option<InnerVec<u8>>, u8> {
        if self.consumed {
            return Ok(None);
        }
        self.consumed = true;
        Ok(Some(unsafe {
            InnerVec::from_raw_parts(self.buffer.as_mut_ptr(), self.buffer.len(), self.buffer.len())
        }))
    }
    fn return_(&mut self, _chunk: InnerVec<u8>) {}
}

/// Backs guest linear memory (unused here: the module declares no memory).
struct GuestMemoryAllocator;
impl WasmMemoryAllocator for GuestMemoryAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        unsafe { NonNull::new(std::alloc::alloc(layout)).ok_or(AllocError::AllocationFailed) }
    }
    fn reallocate(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        unsafe {
            NonNull::new(std::alloc::realloc(ptr.as_ptr(), old, new.size()))
                .ok_or(AllocError::AllocationFailed)
        }
    }
    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { std::alloc::dealloc(ptr.as_ptr(), layout) }
    }
}

// ---- Wasm binary construction ------------------------------------------------

fn uleb(out: &mut std::vec::Vec<u8>, mut v: u64) {
    loop {
        let mut byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

fn section(out: &mut std::vec::Vec<u8>, id: u8, body: &[u8]) {
    out.push(id);
    uleb(out, body.len() as u64);
    out.extend_from_slice(body);
}

fn func_body(func_index: u32) -> std::vec::Vec<u8> {
    let mut b = std::vec![0x00u8]; // 0 local declarations
    if func_index == ENTRY {
        b.push(0x10); // call
        uleb(&mut b, TARGET as u64); // immediate: the raw FuncIdx 65536
    } else {
        let k: i32 = if func_index == TARGET {
            TARGET_RET
        } else if func_index == VICTIM {
            VICTIM_RET
        } else {
            0
        };
        b.push(0x41); // i32.const
        b.push(k as u8); // k < 64: single-byte signed LEB128
    }
    b.push(0x0b); // end
    b
}

fn build_module() -> std::vec::Vec<u8> {
    let mut m = std::vec::Vec::new();
    m.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d]); // magic
    m.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // version

    // Type section: one type () -> i32
    let mut types = std::vec::Vec::new();
    uleb(&mut types, 1);
    types.extend_from_slice(&[0x60, 0x00, 0x01, 0x7f]);
    section(&mut m, 1, &types);

    // Function section: N_FUNCS entries, all type 0
    let mut funcs = std::vec::Vec::new();
    uleb(&mut funcs, N_FUNCS as u64);
    for _ in 0..N_FUNCS {
        funcs.push(0x00);
    }
    section(&mut m, 3, &funcs);

    // Export section: "entry" -> func ENTRY
    let mut exports = std::vec::Vec::new();
    uleb(&mut exports, 1);
    let name = b"entry";
    uleb(&mut exports, name.len() as u64);
    exports.extend_from_slice(name);
    exports.push(0x00); // func
    uleb(&mut exports, ENTRY as u64);
    section(&mut m, 7, &exports);

    // Code section: N_FUNCS bodies
    let mut code = std::vec::Vec::new();
    uleb(&mut code, N_FUNCS as u64);
    for i in 0..N_FUNCS {
        let body = func_body(i);
        uleb(&mut code, body.len() as u64);
        code.extend_from_slice(&body);
    }
    section(&mut m, 10, &code);

    m
}

#[test]
#[cfg_attr(miri, ignore = "builds a large module; far too slow under Miri")]
fn funcidx_truncation_poc() {
    let wasm = build_module();
    eprintln!("built module: {} functions, {} bytes", N_FUNCS, wasm.len());

    let mut engine = Engine::new(1024, 4, spacewasm::vec![]).unwrap();
    let mut code_builder = CodeBuilder::new(CompilerOptions {
        allow_memory_grow: false,
        max_backpatch_iterations: None,
        max_code_pages: (1 << 24) - 1,
    })
    .unwrap();
    let mut stream = ByteStream {
        buffer: wasm,
        consumed: false,
    };

    let module = match Module::new::<MAX_CONTROL_FRAMES, MAX_STACK_DEPTH>(
        "poc",
        &mut stream,
        &mut engine.store,
        &mut code_builder,
        spacewasm::Rc::new(GuestMemoryAllocator)
            .unwrap()
            .into_wasm_memory_allocator(),
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("VERDICT: module REJECTED at load/validation: {e:?}");
            eprintln!("  -> no truncation reachable on this configuration (safe).");
            return;
        }
    };
    eprintln!("module validated and loaded (the spec says it should be rejected)");

    let module_ref = engine.push_module(module).unwrap();

    // Resolve the exported "entry" function (index ENTRY == 1, no truncation).
    let f_ref = {
        let m = &engine.store.modules()[module_ref.0 as usize];
        let export = m
            .exports
            .iter()
            .find(|e| e.name == "entry")
            .expect("entry export");
        let idx = match export.desc {
            ExportDesc::Func(i) => i,
            _ => panic!("entry is not a function"),
        };
        match m.get_func_ref(idx).expect("entry resolves") {
            Ref::Module(index) => WasmRef {
                module: ModuleRef(module_ref.0),
                index,
            },
            other => panic!("unexpected ref {other:?}"),
        }
    };

    engine.invoke(f_ref, &[]).unwrap();
    let result = Interpreter.run(code_builder.pages(), &mut engine, 10_000_000);

    match result {
        InterpreterResult::Finished => {
            let got = match engine.result.unwrap().to_value(ValType::I32) {
                Value::I32(x) => x,
                other => panic!("unexpected return {other:?}"),
            };
            eprintln!("entry() returned {got}");
            if got == VICTIM_RET {
                eprintln!(
                    "VERDICT: TRUNCATION CONFIRMED. `call {TARGET}` ran function {VICTIM} \
                     (= {TARGET} & 0xFFFF), returning {VICTIM_RET} instead of the intended \
                     {TARGET_RET}."
                );
            } else if got == TARGET_RET {
                eprintln!("VERDICT: called correctly (returned {TARGET_RET}); no truncation.");
            } else {
                eprintln!("VERDICT: unexpected value {got}");
            }
            assert_eq!(
                got, VICTIM_RET,
                "expected the mis-dispatched (truncated) target to run"
            );
        }
        other => {
            eprintln!("VERDICT: execution did not finish: {other:?}");
            panic!("entry() did not finish: {other:?}");
        }
    }
}
