use crate::{JumpTarget, Memory, ModuleRef, RawValue, Rc, ResultType, Stack, Store, TableElement};

/// The Wasm engine holds the state of the interpreter and the WebAssembly store
///
/// Refinement: `stack` is a `Stack[base, addr, bytes, size]` (see [`Stack`]), `sp`
/// and `fp` are the stack/frame pointers in words. Invariant `fp <= sp <= size`:
/// the current frame starts at `fp`, the operand stack top is `sp`, and both lie
/// within the `size`-word stack buffer. Operand-stack depth and headroom
/// requirements of individual instructions are expressed through the
/// `has_depth` / `has_room` associated refinements of [`crate::BaseVisitor`].
/// `module` and `store` expose the active [`ModuleRef`] and the [`Store`]
/// indices so that store lookups can require `module.idx < store.modules.len`
/// (not an invariant: `Engine::new` starts at `ModuleRef(0)` on an empty store;
/// it is a precondition of `call_impl`/`invoke` and the store-touching ops).
/// `size <= u32::MAX` makes `fp = sp as u32` (`call_impl`) lossless.
#[flux_rs::refined_by(base: int, addr: int, bytes: int, size: int, sp: int, fp: int, module: ModuleRef, store: Store)]
#[flux_rs::invariant(fp <= sp && sp <= size && size <= 4294967295)]
pub struct Engine {
    /// Current program counter
    pub pc: JumpTarget,

    /// Flag tracking if the current instruction jumps to another PC
    pub jumped: bool,

    /// Frame pointer. Base address of local variables in the current stack frame
    #[flux_rs::field(u32[fp])]
    pub fp: u32,

    /// Stack pointer
    #[flux_rs::field(usize[sp])]
    pub sp: usize,

    /// The stack
    #[flux_rs::field(Stack[base, addr, bytes, size])]
    pub stack: Stack,

    /// Linear memory from the active module
    pub memory: Rc<Memory>,

    /// Table from the active module
    pub table: Rc<[TableElement]>,

    /// Current module we are executing code for
    #[flux_rs::field(ModuleRef[module])]
    pub module: ModuleRef,

    /// The WebAssembly Store
    #[flux_rs::field(Store[store])]
    pub store: Store,

    /// A host function requested a pause.
    /// This field will track the result type expected from an interpreter resume.
    pub host_pause_result: Option<ResultType>,

    /// The interpreter result when finished executing
    pub result: Option<RawValue>,
}
