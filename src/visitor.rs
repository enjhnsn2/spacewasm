use crate::{
    FuncIdx, GlobalIdx, LabelIdx, LabelTarget, LocalIdx, MemArg, ResultType, TypeIdx, ValType,
};

/// A convenience macro for defining the visitor function for a decoded
/// WebAssembly instruction from any intermediate representation.
///
/// `[$pops, $pushes]` is the instruction's operand-stack effect in 32-bit words.
/// It is turned into a flux precondition expressed through the visitor's
/// associated refinements `has_depth` / `has_room` (which default to `true`, so
/// visitors that do not model a stack are unaffected).
macro_rules! visit_fn {
    // No additional parameters
    ($name:ident [$pops:tt, $pushes:tt]) => {
        #[flux_rs::spec(fn(&Self, state: &strg Self::State[@s]) -> Result<(), Self::Error>
            requires <Self as BaseVisitor>::has_depth(s, $pops) && <Self as BaseVisitor>::has_room(s, $pops, $pushes)
            ensures state: Self::State)]
        fn $name(&self, state: &mut Self::State) -> Result<(), Self::Error>;
    };

    // With additional parameters
    ($name:ident [$pops:tt, $pushes:tt], $($param:ident : $ty:tt),+) => {
        #[flux_rs::spec(fn(&Self, $($param: $ty),+, state: &strg Self::State[@s]) -> Result<(), Self::Error>
            requires <Self as BaseVisitor>::has_depth(s, $pops) && <Self as BaseVisitor>::has_room(s, $pops, $pushes)
            ensures state: Self::State)]
        fn $name(&self, $($param: $ty),+, state: &mut Self::State) -> Result<(), Self::Error>;
    };
}

/// An abstraction over Wasm IR and internal IR.
/// This trait can be used to index, compile and execute either form of IR
/// with the same common implementation. The decoding and traversal code will
/// call into this visitor and is IR specific. This trait is purely for operating
/// on decoded WebAssembly instructions.
///
/// Note: This visitor does not handle the control-flow instructions since those are
///       IR-specific. See [WasmVisitor] and [IrVisitor]
#[flux_rs::assoc(
    fn has_depth(s: Self::State, n: int) -> bool { true }
    fn has_room(s: Self::State, pops: int, pushes: int) -> bool { true }
)]
pub trait BaseVisitor {
    type Error;
    type State;

    // Control instructions
    visit_fn!(unreachable [0, 0]);
    visit_fn!(nop [0, 0]);

    /// The decoder reached an opcode or immediate it cannot interpret.
    fn malformed_ir(&self, state: &mut Self::State) -> Result<(), Self::Error> {
        self.unreachable(state)
    }

    // Control flow & parametric instructions are not handled by the base visitor

    // Memory instructions - loads
    visit_fn!(i32_load [1, 1], m: MemArg);
    visit_fn!(i64_load [1, 2], m: MemArg);
    visit_fn!(f32_load [1, 1], m: MemArg);
    visit_fn!(f64_load [1, 2], m: MemArg);
    visit_fn!(i32_load8_s [1, 1], m: MemArg);
    visit_fn!(i32_load8_u [1, 1], m: MemArg);
    visit_fn!(i32_load16_s [1, 1], m: MemArg);
    visit_fn!(i32_load16_u [1, 1], m: MemArg);
    visit_fn!(i64_load8_s [1, 2], m: MemArg);
    visit_fn!(i64_load8_u [1, 2], m: MemArg);
    visit_fn!(i64_load16_s [1, 2], m: MemArg);
    visit_fn!(i64_load16_u [1, 2], m: MemArg);
    visit_fn!(i64_load32_s [1, 2], m: MemArg);
    visit_fn!(i64_load32_u [1, 2], m: MemArg);

    // Memory instructions - stores
    visit_fn!(i32_store [2, 0], m: MemArg);
    visit_fn!(i64_store [3, 0], m: MemArg);
    visit_fn!(f32_store [2, 0], m: MemArg);
    visit_fn!(f64_store [3, 0], m: MemArg);
    visit_fn!(i32_store8 [2, 0], m: MemArg);
    visit_fn!(i32_store16 [2, 0], m: MemArg);
    visit_fn!(i64_store8 [3, 0], m: MemArg);
    visit_fn!(i64_store16 [3, 0], m: MemArg);
    visit_fn!(i64_store32 [3, 0], m: MemArg);

    // Memory instructions - size/grow
    visit_fn!(memory_size [0, 1]);
    visit_fn!(memory_grow [1, 1]);

    // Numeric instructions - const
    visit_fn!(i32_const [0, 1], n: i32);
    visit_fn!(i64_const [0, 2], n: i64);
    visit_fn!(f32_const [0, 1], z: f32);
    visit_fn!(f64_const [0, 2], z: f64);

    // Numeric instructions - i32 test/rel
    visit_fn!(i32_eqz [1, 1]);
    visit_fn!(i32_eq [2, 1]);
    visit_fn!(i32_ne [2, 1]);
    visit_fn!(i32_lt_s [2, 1]);
    visit_fn!(i32_lt_u [2, 1]);
    visit_fn!(i32_gt_s [2, 1]);
    visit_fn!(i32_gt_u [2, 1]);
    visit_fn!(i32_le_s [2, 1]);
    visit_fn!(i32_le_u [2, 1]);
    visit_fn!(i32_ge_s [2, 1]);
    visit_fn!(i32_ge_u [2, 1]);

    // Numeric instructions - i64 test/rel
    visit_fn!(i64_eqz [2, 1]);
    visit_fn!(i64_eq [4, 1]);
    visit_fn!(i64_ne [4, 1]);
    visit_fn!(i64_lt_s [4, 1]);
    visit_fn!(i64_lt_u [4, 1]);
    visit_fn!(i64_gt_s [4, 1]);
    visit_fn!(i64_gt_u [4, 1]);
    visit_fn!(i64_le_s [4, 1]);
    visit_fn!(i64_le_u [4, 1]);
    visit_fn!(i64_ge_s [4, 1]);
    visit_fn!(i64_ge_u [4, 1]);

    // Numeric instructions - f32 rel
    visit_fn!(f32_eq [2, 1]);
    visit_fn!(f32_ne [2, 1]);
    visit_fn!(f32_lt [2, 1]);
    visit_fn!(f32_gt [2, 1]);
    visit_fn!(f32_le [2, 1]);
    visit_fn!(f32_ge [2, 1]);

    // Numeric instructions - f64 rel
    visit_fn!(f64_eq [4, 1]);
    visit_fn!(f64_ne [4, 1]);
    visit_fn!(f64_lt [4, 1]);
    visit_fn!(f64_gt [4, 1]);
    visit_fn!(f64_le [4, 1]);
    visit_fn!(f64_ge [4, 1]);

    // Numeric instructions - i32 unary/binary
    visit_fn!(i32_clz [1, 1]);
    visit_fn!(i32_ctz [1, 1]);
    visit_fn!(i32_popcnt [1, 1]);
    visit_fn!(i32_add [2, 1]);
    visit_fn!(i32_sub [2, 1]);
    visit_fn!(i32_mul [2, 1]);
    visit_fn!(i32_div_s [2, 1]);
    visit_fn!(i32_div_u [2, 1]);
    visit_fn!(i32_rem_s [2, 1]);
    visit_fn!(i32_rem_u [2, 1]);
    visit_fn!(i32_and [2, 1]);
    visit_fn!(i32_or [2, 1]);
    visit_fn!(i32_xor [2, 1]);
    visit_fn!(i32_shl [2, 1]);
    visit_fn!(i32_shr_s [2, 1]);
    visit_fn!(i32_shr_u [2, 1]);
    visit_fn!(i32_rotl [2, 1]);
    visit_fn!(i32_rotr [2, 1]);

    // Numeric instructions - i64 unary/binary
    visit_fn!(i64_clz [2, 2]);
    visit_fn!(i64_ctz [2, 2]);
    visit_fn!(i64_popcnt [2, 2]);
    visit_fn!(i64_add [4, 2]);
    visit_fn!(i64_sub [4, 2]);
    visit_fn!(i64_mul [4, 2]);
    visit_fn!(i64_div_s [4, 2]);
    visit_fn!(i64_div_u [4, 2]);
    visit_fn!(i64_rem_s [4, 2]);
    visit_fn!(i64_rem_u [4, 2]);
    visit_fn!(i64_and [4, 2]);
    visit_fn!(i64_or [4, 2]);
    visit_fn!(i64_xor [4, 2]);
    visit_fn!(i64_shl [4, 2]);
    visit_fn!(i64_shr_s [4, 2]);
    visit_fn!(i64_shr_u [4, 2]);
    visit_fn!(i64_rotl [4, 2]);
    visit_fn!(i64_rotr [4, 2]);

    // Numeric instructions - f32 unary/binary
    visit_fn!(f32_abs [1, 1]);
    visit_fn!(f32_neg [1, 1]);
    visit_fn!(f32_ceil [1, 1]);
    visit_fn!(f32_floor [1, 1]);
    visit_fn!(f32_trunc [1, 1]);
    visit_fn!(f32_nearest [1, 1]);
    visit_fn!(f32_sqrt [1, 1]);
    visit_fn!(f32_add [2, 1]);
    visit_fn!(f32_sub [2, 1]);
    visit_fn!(f32_mul [2, 1]);
    visit_fn!(f32_div [2, 1]);
    visit_fn!(f32_min [2, 1]);
    visit_fn!(f32_max [2, 1]);
    visit_fn!(f32_copysign [2, 1]);

    // Numeric instructions - f64 unary/binary
    visit_fn!(f64_abs [2, 2]);
    visit_fn!(f64_neg [2, 2]);
    visit_fn!(f64_ceil [2, 2]);
    visit_fn!(f64_floor [2, 2]);
    visit_fn!(f64_trunc [2, 2]);
    visit_fn!(f64_nearest [2, 2]);
    visit_fn!(f64_sqrt [2, 2]);
    visit_fn!(f64_add [4, 2]);
    visit_fn!(f64_sub [4, 2]);
    visit_fn!(f64_mul [4, 2]);
    visit_fn!(f64_div [4, 2]);
    visit_fn!(f64_min [4, 2]);
    visit_fn!(f64_max [4, 2]);
    visit_fn!(f64_copysign [4, 2]);

    // Numeric instructions - conversions
    visit_fn!(i32_wrap_i64 [2, 1]);
    visit_fn!(i32_trunc_f32_s [1, 1]);
    visit_fn!(i32_trunc_f32_u [1, 1]);
    visit_fn!(i32_trunc_f64_s [2, 1]);
    visit_fn!(i32_trunc_f64_u [2, 1]);
    visit_fn!(i64_extend_i32_s [1, 2]);
    visit_fn!(i64_extend_i32_u [1, 2]);
    visit_fn!(i64_trunc_f32_s [1, 2]);
    visit_fn!(i64_trunc_f32_u [1, 2]);
    visit_fn!(i64_trunc_f64_s [2, 2]);
    visit_fn!(i64_trunc_f64_u [2, 2]);
    visit_fn!(f32_convert_i32_s [1, 1]);
    visit_fn!(f32_convert_i32_u [1, 1]);
    visit_fn!(f32_convert_i64_s [2, 1]);
    visit_fn!(f32_convert_i64_u [2, 1]);
    visit_fn!(f32_demote_f64 [2, 1]);
    visit_fn!(f64_convert_i32_s [1, 2]);
    visit_fn!(f64_convert_i32_u [1, 2]);
    visit_fn!(f64_convert_i64_s [2, 2]);
    visit_fn!(f64_convert_i64_u [2, 2]);
    visit_fn!(f64_promote_f32 [1, 2]);
}

/// An abstraction over Wasm Bytecode.
/// Used to implement validation and compilation of Wasm bytecode.
pub trait WasmVisitor: BaseVisitor {
    // Parametric instructions
    visit_fn!(drop [0, 0]);
    visit_fn!(select [0, 0]);

    visit_fn!(enter_block [0, 0], block_type: ResultType);
    visit_fn!(exit_block [0, 0]);
    visit_fn!(finish [0, 0]);
    visit_fn!(loop_ [0, 0], block_type: ResultType);
    visit_fn!(if_ [0, 0], block_type: ResultType);
    visit_fn!(else_ [0, 0]);
    visit_fn!(br [0, 0], l: LabelIdx);
    visit_fn!(br_if [0, 0], l: LabelIdx);
    visit_fn!(br_table_start [0, 0], len: u32);
    visit_fn!(br_table_branch [0, 0], br: LabelIdx);
    visit_fn!(br_table_finish [0, 0], default_: LabelIdx);

    visit_fn!(return_ [0, 0]);
    visit_fn!(call [0, 0], x: FuncIdx);
    visit_fn!(call_indirect [0, 0], x: TypeIdx);

    // Variable instructions
    visit_fn!(local_get [0, 0], x: LocalIdx);
    visit_fn!(local_set [0, 0], x: LocalIdx);
    visit_fn!(local_tee [0, 0], x: LocalIdx);
    visit_fn!(global_get [0, 0], x: GlobalIdx);
    visit_fn!(global_set [0, 0], x: GlobalIdx);

    // Validation instructions
    visit_fn!(i32_reinterpret_f32 [0, 0]);
    visit_fn!(i64_reinterpret_f64 [0, 0]);
    visit_fn!(f32_reinterpret_i32 [0, 0]);
    visit_fn!(f64_reinterpret_i64 [0, 0]);
}

#[derive(Debug)]
#[flux_rs::refined_by(off: int)]
pub struct LocalVariable {
    // Offset of the local variable in 32-bit words
    // Function parameters are negative relative to the FP
    // Locals are positive relative to FP + 2
    #[flux_rs::field(i16[off])]
    pub frame_offset: i16,

    // Variable's type
    pub ty: ValType,
}

/// An index offset from the current module.
/// Module imports can only refer to modules loaded before it.
/// References store a relative offset from the 'current' module. 0 indicates the current module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRef(pub u8);

#[derive(Debug, Clone, Copy)]
pub struct HostModuleRef(pub u8);

impl HostModuleRef {
    /// Construct a new module reference given the absolute index to the module and the store.
    pub fn new(module_index: usize) -> HostModuleRef {
        HostModuleRef(module_index as u8)
    }
}

/// A reference to a symbol in the Wasm store relative to an implicit 'self' module
#[derive(Debug, Clone, Copy)]
pub enum Ref {
    /// A symbol in the current Wasm module
    Module(u16),
    /// A symbol in an external host module
    Host { module: HostModuleRef, index: u16 },
    /// A symbol in another Wasm module
    Extern { module: ModuleRef, index: u16 },
}

/// An abstraction over IR Bytecode.
/// Used to implement the interpreter.
#[flux_rs::assoc(
    fn has_local(s: Self::State, l: LocalVariable) -> bool { true }
    fn can_branch(s: Self::State, l: LabelTarget) -> bool { true }
    fn can_branch_after_pop(s: Self::State, l: LabelTarget) -> bool { true }
)]
pub trait IrVisitor: BaseVisitor {
    // Parametric instructions
    visit_fn!(drop [2, 0], ty: ValType);
    visit_fn!(select [5, 2], ty: ValType);

    visit_fn!(if_ [1, 0], false_address: LabelTarget);
    #[flux_rs::spec(fn(&Self, addr: LabelTarget, state: &strg Self::State[@s]) -> Result<(), Self::Error>
        requires <Self as IrVisitor>::can_branch(s, addr)
        ensures state: Self::State)]
    fn br(&self, addr: LabelTarget, state: &mut Self::State) -> Result<(), Self::Error>;
    #[flux_rs::spec(fn(&Self, true_address: LabelTarget, state: &strg Self::State[@s]) -> Result<(), Self::Error>
        requires <Self as BaseVisitor>::has_depth(s, 1) && <Self as IrVisitor>::can_branch_after_pop(s, true_address)
        ensures state: Self::State)]
    fn br_if(&self, true_address: LabelTarget, state: &mut Self::State) -> Result<(), Self::Error>;
    // No spec: flux rejects signatures with closure bounds. The interpreter's
    // implementation delegates to `br_impl`, which carries the real precondition.
    fn br_table(
        &self,
        n: u32,
        cases: impl FnOnce(u32) -> LabelTarget,
        state: &mut Self::State,
    ) -> Result<(), Self::Error>;

    #[flux_rs::spec(fn(&Self, return_size: u8, state: &strg Self::State[@s]) -> Result<(), Self::Error>
        requires <Self as BaseVisitor>::has_depth(s, return_size + 2)
        ensures state: Self::State)]
    fn return_(&self, return_size: u8, state: &mut Self::State) -> Result<(), Self::Error>;
    visit_fn!(call [0, 0], x: u16);
    visit_fn!(call_host [0, 0], module: HostModuleRef, x: u16);
    visit_fn!(call_extern [0, 0], module: ModuleRef, x: u16);
    visit_fn!(call_indirect [1, 0], x: TypeIdx);

    // Variable instructions
    #[flux_rs::spec(fn(&Self, l: LocalVariable, state: &strg Self::State[@s]) -> Result<(), Self::Error>
        requires <Self as BaseVisitor>::has_room(s, 0, 1) && <Self as IrVisitor>::has_local(s, l)
        ensures state: Self::State)]
    fn local_get(&self, l: LocalVariable, state: &mut Self::State) -> Result<(), Self::Error>;
    #[flux_rs::spec(fn(&Self, l: LocalVariable, state: &strg Self::State[@s]) -> Result<(), Self::Error>
        requires <Self as BaseVisitor>::has_depth(s, 2) && <Self as IrVisitor>::has_local(s, l)
        ensures state: Self::State)]
    fn local_set(&self, l: LocalVariable, state: &mut Self::State) -> Result<(), Self::Error>;
    #[flux_rs::spec(fn(&Self, l: LocalVariable, state: &strg Self::State[@s]) -> Result<(), Self::Error>
        requires <Self as BaseVisitor>::has_depth(s, 2) && <Self as IrVisitor>::has_local(s, l)
        ensures state: Self::State)]
    fn local_tee(&self, l: LocalVariable, state: &mut Self::State) -> Result<(), Self::Error>;
    visit_fn!(global_get [0, 1], idx: u16);
    visit_fn!(global_set [2, 0], idx: u16);
    visit_fn!(global_get_host [0, 1], module: HostModuleRef, index: u16);
    visit_fn!(global_set_host [2, 0], module: HostModuleRef, index: u16);
    visit_fn!(global_get_extern [0, 1], module: ModuleRef, index: u16);
    visit_fn!(global_set_extern [2, 0], module: ModuleRef, index: u16);
}
