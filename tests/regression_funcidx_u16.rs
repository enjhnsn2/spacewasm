//! Regression test for `FuncIdx` truncation in `Module::get_func_ref`.
//!
//! `get_func_ref` resolved a module-local function index with `i as u16` while
//! only checking `i < functions.len()`. A module with more than 65536 functions
//! could therefore name a function whose local index exceeds `u16::MAX`; the
//! cast wrapped it and the call was silently dispatched to `index & 0xFFFF`,
//! even though the module passed validation. A module that cannot be represented
//! must be rejected instead.
//!
//! This test builds a module with 65537 functions whose function 0 contains
//! `call 65536` and asserts the module is rejected with `FunctionIdxOutOfRange`.
//! It also pins the boundary: a `call` to local index 65535 (`u16::MAX`) in a
//! 65536-function module still validates.

use core::alloc::Layout;

use spacewasm::{
    AllocError, Allocator, CodeBuilder, CompilerOptions, Engine, InnerVec, Module, ValidationError,
    WasmMemoryAllocator, WasmStream, global_allocator,
};

extern crate std;

const MAX_CONTROL_FRAMES: usize = 128;
const MAX_STACK_DEPTH: usize = 256;

struct SystemAllocator;
unsafe impl Allocator for SystemAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        let p = unsafe { std::alloc::alloc(layout) };
        if p.is_null() { Err(AllocError::AllocationFailed) } else { Ok(p) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { std::alloc::dealloc(ptr, layout) }
    }
}
global_allocator!(SystemAllocator, SystemAllocator);

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

struct GuestMemoryAllocator;
impl WasmMemoryAllocator for GuestMemoryAllocator {
    fn allocate(&self, layout: Layout) -> Result<core::ptr::NonNull<u8>, AllocError> {
        unsafe { core::ptr::NonNull::new(std::alloc::alloc(layout)).ok_or(AllocError::AllocationFailed) }
    }
    fn reallocate(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<core::ptr::NonNull<u8>, AllocError> {
        unsafe {
            core::ptr::NonNull::new(std::alloc::realloc(ptr.as_ptr(), old, new.size()))
                .ok_or(AllocError::AllocationFailed)
        }
    }
    fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: Layout) {
        unsafe { std::alloc::dealloc(ptr.as_ptr(), layout) }
    }
}

fn uleb(out: &mut std::vec::Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
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

/// A module of `n_funcs` functions, all `() -> ()`, where function 0 is
/// `call call_target; end` and every other function is empty.
fn module_with_call(n_funcs: u32, call_target: u32) -> std::vec::Vec<u8> {
    let mut m = std::vec::Vec::new();
    m.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    let mut types = std::vec::Vec::new();
    uleb(&mut types, 1);
    types.extend_from_slice(&[0x60, 0x00, 0x00]); // () -> ()
    section(&mut m, 1, &types);

    let mut funcs = std::vec::Vec::new();
    uleb(&mut funcs, n_funcs as u64);
    for _ in 0..n_funcs {
        funcs.push(0x00);
    }
    section(&mut m, 3, &funcs);

    let mut code = std::vec::Vec::new();
    uleb(&mut code, n_funcs as u64);
    for i in 0..n_funcs {
        let mut body = std::vec::Vec::new();
        body.push(0x00); // 0 local declarations
        if i == 0 {
            body.push(0x10); // call
            uleb(&mut body, call_target as u64);
        }
        body.push(0x0b); // end
        uleb(&mut code, body.len() as u64);
        code.extend_from_slice(&body);
    }
    section(&mut m, 10, &code);
    m
}

fn decode(wasm: std::vec::Vec<u8>) -> Result<(), ValidationError> {
    let mut engine = Engine::new(1024, 4, spacewasm::vec![]).unwrap();
    let mut code_builder = CodeBuilder::new(CompilerOptions {
        allow_memory_grow: false,
        max_backpatch_iterations: None,
        max_code_pages: (1 << 24) - 1,
    })
    .unwrap();
    let mut stream = ByteStream { buffer: wasm, consumed: false };
    Module::new::<MAX_CONTROL_FRAMES, MAX_STACK_DEPTH>(
        "t",
        &mut stream,
        &mut engine.store,
        &mut code_builder,
        spacewasm::Rc::new(GuestMemoryAllocator).unwrap().into_wasm_memory_allocator(),
    )
    .map(|_| ())
    .map_err(|e| e.err.err)
}

/// A `call` to a local index that does not fit a `u16` must be rejected, not
/// silently truncated to `index & 0xFFFF`.
#[test]
fn call_to_index_above_u16_is_rejected() {
    // 65537 functions (indices 0..=65536); function 0 calls index 65536.
    let result = decode(module_with_call(65537, 65536));
    assert!(
        matches!(result, Err(ValidationError::FunctionIdxOutOfRange)),
        "expected FunctionIdxOutOfRange, got {result:?}"
    );
}

/// Boundary: a `call` to the largest index a `u16` can hold (65535) still
/// validates, so the fix does not over-reject.
#[test]
fn call_to_max_u16_index_is_accepted() {
    // 65536 functions (indices 0..=65535); function 0 calls index 65535.
    let result = decode(module_with_call(65536, 65535));
    assert!(result.is_ok(), "expected the module to validate, got {result:?}");
}
