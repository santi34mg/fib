use crate::{
    frontend::{
        tokens::{builtin::BuiltinFunction, name::Name},
        typed_ast::{bin_op::BinOp, ty::Ty},
    },
    ir::{Label, Operand, Temp, UnOp},
};

#[derive(Debug, Clone)]
pub enum Instruction {
    /// Allocate a named slot. Emitted once per `Binding` (including params).
    Alloca {
        sym: Name,
        name: String,
        ty: Ty,
    },
    Store {
        dst: Name,
        src: Operand,
    },
    Load {
        dst: Temp,
        src: Name,
    },
    Copy {
        dst: Temp,
        src: Operand,
    },
    Binary {
        dst: Temp,
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
        /// Operand type (left's) for opcode selection: float vs int and
        /// signed vs unsigned (`SDiv` vs `UDiv`, `SLT` vs `ULT`, ...).
        /// For comparisons the result temp holds `@bool`.
        ty: Ty,
    },
    Unary {
        dst: Temp,
        op: UnOp,
        src: Operand,
        ty: Ty,
    },
    AddrOf {
        dst: Temp,
        src: Name,
    },
    LoadPtr {
        dst: Temp,
        ptr: Operand,
        ty: Ty,
    },
    StorePtr {
        ptr: Operand,
        src: Operand,
    },
    Cast {
        dst: Temp,
        src: Operand,
        from: Ty,
        target: Ty,
    },
    Call {
        dst: Option<Temp>,
        callee: String,
        args: Vec<Operand>,
        ret_ty: Ty,
        is_variadic: bool,
    },
    BuiltinCall {
        dst: Option<Temp>,
        builtin: BuiltinFunction,
        args: Vec<Operand>,
        ret_ty: Ty,
    },
    // --- aggregates / enums (design-locked, lowering lands in phase 2b) ---
    StructConstruct {
        dst: Temp,
        type_name: String,
        fields: Vec<(String, Operand)>,
        ty: Ty,
    },
    FieldLoad {
        dst: Temp,
        base: Operand,
        field_index: usize,
        ty: Ty,
    },
    FieldStore {
        base: Operand,
        field_index: usize,
        src: Operand,
    },
    ArrayLiteral {
        dst: Temp,
        elems: Vec<Operand>,
        ty: Ty,
    },
    IndexLoad {
        dst: Temp,
        base: Operand,
        index: Operand,
        ty: Ty,
    },
    IndexStore {
        base: Operand,
        index: Operand,
        src: Operand,
    },
    EnumLiteral {
        dst: Temp,
        discriminant: u32,
        ty: Ty,
    },
    EnumConstruct {
        dst: Temp,
        discriminant: u32,
        payload: Vec<(String, Operand)>,
        ty: Ty,
    },
    SwitchDispatch {
        subject: Operand,
        /// (discriminant, target) pairs; backend emits `switch` on the tag.
        cases: Vec<(u32, Label)>,
        default: Label,
    },
    TupleExtract {
        dst: Temp,
        src: Operand,
        index: usize,
        ty: Ty,
    },
    // --- control flow (flat only) ---
    IfGoto {
        cond: Operand,
        target: Label,
    },
    Goto {
        target: Label,
    },
    LabelDef(Label),
    Return {
        values: Vec<Operand>,
    },
    Unreachable,
}
