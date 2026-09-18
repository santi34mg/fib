//! Dragon-style three-address IR (middle-end).
//!
//! Boundary: the frontend produces `typed_ast::TypedProgram` (trees +
//! `If`/`For`/`Switch`, nested expressions). This module is the first
//! flat representation: at most one operation per instruction, control
//! flow only via labels and jumps, values in temporaries.
//!
//! Status: `lower_typed_program` implements the core subset (locals,
//! integer/bool/float arithmetic, calls, `if`/`for`/`break`/`continue`,
//! `defer` via inline duplication at `return`/`break`/`continue`, `return`).
//! Aggregate / pointer / enum / `switch` / multi-value nodes are modeled
//! as explicit `Instruction` variants (design lock) but return
//! `LowerError::Unsupported` until phase 2b. `backend::lowering` still
//! consumes the typed AST directly; migration to consume `IrProgram`
//! happens after parity tests pass.

use std::collections::HashMap;
use std::fmt;

use crate::frontend::tokens::Operator;
use crate::frontend::tokens::builtin::BuiltinFunction;
use crate::frontend::typed_ast::{
    Ty, TypedDecl, TypedExpr, TypedExprKind, TypedProgram, TypedStatement,
};

/// A jump target. Labels are patched after emission (Dragon backpatching),
/// so jumps refer to a `Label` id, never to an `Instruction` by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(pub u32);

/// A compiler-generated temporary holding one flat value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Temp(pub u32);

/// A resolved source binding (one `Alloca` slot). Unlike the old
/// `Place(String)`, this survives shadowing: each `Binding` allocates a
/// fresh `SymbolId`, and lexical scopes map names to the live id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// An operand: a temporary, a resolved binding, or a constant.
#[derive(Debug, Clone)]
pub enum Operand {
    Temp(Temp),
    Symbol(SymbolId),
    ConstInt { value: u64, ty: Ty },
    ConstFloat { value: f64, ty: Ty },
    ConstBool(bool),
    StringLit(String),
    Null,
}

/// Generic binary operation. Signedness / float-ness is carried by the
/// `ty` field on `Instruction::Binary` so the backend can pick
/// `SDiv` vs `UDiv`, `SLT` vs `ULT`, `LShr` vs `AShr`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    /// Allocate a named slot. Emitted once per `Binding` (including params).
    Alloca {
        sym: SymbolId,
        name: String,
        ty: Ty,
    },
    Store {
        dst: SymbolId,
        src: Operand,
    },
    Load {
        dst: Temp,
        src: SymbolId,
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
        src: SymbolId,
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

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub label: Label,
    pub instrs: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(SymbolId, String, Ty)>,
    pub return_ty: Ty,
    pub is_extern: bool,
    pub is_variadic: bool,
    pub temps: u32,
    pub symbols: Vec<(SymbolId, String, Ty)>,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    Unsupported(String),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "IR lowering unsupported: {}", msg),
        }
    }
}

impl std::error::Error for LowerError {}

// ---------------------------------------------------------------------------
// Predicates shared with the backend (single source of truth for signedness)
// ---------------------------------------------------------------------------

/// True for `@uint*` types. The backend uses this to pick `UDiv`/`URem`,
/// `ULT/UGT/...`, logical `LShr`, and `zext`/`uitofp` over signed forms.
pub fn ty_is_unsigned(ty: &Ty) -> bool {
    use crate::frontend::tokens::builtin::BuiltinType;
    matches!(
        ty,
        Ty::Builtin(
            BuiltinType::UInt1
                | BuiltinType::UInt2
                | BuiltinType::UInt4
                | BuiltinType::UInt8
                | BuiltinType::UInt16
        )
    )
}

pub fn ty_is_float(ty: &Ty) -> bool {
    use crate::frontend::tokens::builtin::BuiltinType;
    matches!(
        ty,
        Ty::Builtin(
            BuiltinType::Float2 | BuiltinType::Float4 | BuiltinType::Float8 | BuiltinType::Float16
        )
    )
}

fn operator_to_binop(op: &Operator) -> Option<BinOp> {
    match op {
        Operator::Plus => Some(BinOp::Add),
        Operator::Minus => Some(BinOp::Sub),
        Operator::Star => Some(BinOp::Mul),
        Operator::Slash => Some(BinOp::Div),
        Operator::Percent => Some(BinOp::Rem),
        Operator::LeftShift => Some(BinOp::Shl),
        Operator::RightShift => Some(BinOp::Shr),
        Operator::Ampersand => Some(BinOp::And),
        Operator::Pipe => Some(BinOp::Or),
        Operator::Caret => Some(BinOp::Xor),
        Operator::DoubleEquals => Some(BinOp::Eq),
        Operator::Different => Some(BinOp::Ne),
        Operator::LesserThan => Some(BinOp::Lt),
        Operator::LesserEqual => Some(BinOp::Le),
        Operator::GreaterThan => Some(BinOp::Gt),
        Operator::GreaterEqual => Some(BinOp::Ge),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

struct LoopTarget {
    break_label: Label,
    continue_label: Label,
    deferred_depth: usize,
}

struct FunctionBuilder {
    name: String,
    params: Vec<(SymbolId, String, Ty)>,
    return_ty: Ty,
    is_extern: bool,
    is_variadic: bool,
    next_temp: u32,
    next_label: u32,
    next_sym: u32,
    symbols: Vec<(SymbolId, String, Ty)>,
    scopes: Vec<HashMap<String, SymbolId>>,
    instrs: Vec<Instruction>,
    deferred: Vec<Vec<TypedStatement>>,
    loops: Vec<LoopTarget>,
}

impl FunctionBuilder {
    fn new(name: String, return_ty: Ty, is_extern: bool, is_variadic: bool) -> Self {
        Self {
            name,
            params: Vec::new(),
            return_ty,
            is_extern,
            is_variadic,
            next_temp: 0,
            next_label: 0,
            next_sym: 0,
            symbols: Vec::new(),
            scopes: vec![HashMap::new()],
            instrs: Vec::new(),
            deferred: vec![Vec::new()],
            loops: Vec::new(),
        }
    }

    fn fresh_temp(&mut self) -> Temp {
        let t = Temp(self.next_temp);
        self.next_temp += 1;
        t
    }

    fn fresh_label(&mut self) -> Label {
        let l = Label(self.next_label);
        self.next_label += 1;
        l
    }

    fn fresh_symbol(&mut self, name: String, ty: Ty) -> SymbolId {
        let s = SymbolId(self.next_sym);
        self.next_sym += 1;
        self.symbols.push((s, name.clone(), ty));
        self.scopes
            .last_mut()
            .expect("FunctionBuilder has no scope")
            .insert(name, s);
        s
    }

    fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn emit(&mut self, instr: Instruction) {
        self.instrs.push(instr);
    }

    fn finish(mut self) -> IrFunction {
        // Split flat instruction stream into basic blocks at LabelDefs AND
        // terminators (Goto/IfGoto/Return/Unreachable). Each terminator ends
        // its block so the LLVM consumer gets a 1:1 block mapping where
        // IfGoto's false edge is "next block". Implicit labels use fresh ids
        // from the same counter (never colliding with explicit labels).
        fn is_terminator(instr: &Instruction) -> bool {
            matches!(
                instr,
                Instruction::Goto { .. }
                    | Instruction::IfGoto { .. }
                    | Instruction::Return { .. }
                    | Instruction::Unreachable
            )
        }
        let mut blocks: Vec<BasicBlock> = Vec::new();
        let mut cur_label: Option<Label> = None;
        let mut cur_instrs: Vec<Instruction> = Vec::new();
        // Drain to avoid borrow issues with fresh_label (&mut self).
        let instrs = std::mem::take(&mut self.instrs);
        for instr in instrs {
            if let Instruction::LabelDef(l) = instr {
                if !cur_instrs.is_empty() || cur_label.is_some() {
                    blocks.push(BasicBlock {
                        label: cur_label.unwrap_or_else(|| {
                            let l = Label(self.next_label);
                            self.next_label += 1;
                            l
                        }),
                        instrs: std::mem::take(&mut cur_instrs),
                    });
                }
                cur_label = Some(l);
            } else {
                if cur_label.is_none() {
                    cur_label = Some(Label(self.next_label));
                    self.next_label += 1;
                }
                let term = is_terminator(&instr);
                cur_instrs.push(instr);
                if term {
                    blocks.push(BasicBlock {
                        label: cur_label.take().unwrap(),
                        instrs: std::mem::take(&mut cur_instrs),
                    });
                    // Next block starts fresh; its label is assigned when we
                    // see its first instruction or LabelDef.
                }
            }
        }
        if !cur_instrs.is_empty() || cur_label.is_some() {
            blocks.push(BasicBlock {
                label: cur_label.unwrap_or_else(|| {
                    let l = Label(self.next_label);
                    self.next_label += 1;
                    l
                }),
                instrs: cur_instrs,
            });
        }
        IrFunction {
            name: self.name,
            params: self.params,
            return_ty: self.return_ty,
            is_extern: self.is_extern,
            is_variadic: self.is_variadic,
            temps: self.next_temp,
            symbols: self.symbols,
            blocks,
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Lower a typed program to flat IR.
///
/// Handles the core subset; everything else returns
/// `LowerError::Unsupported` with the source construct named.
pub fn lower_typed_program(program: TypedProgram) -> Result<IrProgram, LowerError> {
    let mut out = IrProgram::default();
    let mut decls = program.declarations;
    decls.extend(program.imported_declarations);
    for decl in &decls {
        match decl {
            TypedDecl::Function(f) => {
                out.functions.push(lower_function(f)?);
            }
            TypedDecl::Const(b) => {
                return Err(LowerError::Unsupported(format!(
                    "module-level const '{}' (lower via function bodies first)",
                    b.name
                )));
            }
            TypedDecl::Type(_) => {
                // Types live in the scope; no code to emit.
            }
        }
    }
    Ok(out)
}

fn lower_function(f: &crate::frontend::typed_ast::TypedFunction) -> Result<IrFunction, LowerError> {
    let mut b = FunctionBuilder::new(
        f.name.value.clone(),
        f.return_type.clone(),
        f.is_extern,
        f.is_variadic,
    );
    // Params are allocas like locals so `Store`/`Load` stay uniform.
    // Extern functions get no body: record params but emit nothing.
    for (name, ty) in &f.params {
        let sym = b.fresh_symbol(name.value.clone(), ty.clone());
        b.params.push((sym, name.value.clone(), ty.clone()));
        if !f.is_extern {
            b.emit(Instruction::Alloca {
                sym,
                name: name.value.clone(),
                ty: ty.clone(),
            });
        }
    }
    if f.is_extern {
        // Drop the param symbols from the block stream: externs lower to a
        // declaration only. Clear symbols' allocas by returning a function
        // with no blocks.
        let mut func = b.finish();
        func.blocks.clear();
        func.symbols.clear();
        return Ok(func);
    }
    b.enter_scope();
    // Re-bind params into the function scope frame (fresh_symbol above put
    // them in the outer frame; mirror them here so shadowing works).
    // Note: fresh_symbol already inserted into scopes[0]; re-insert the same
    // ids into the new frame without allocating new slots.
    for (sym, name, _) in b.params.clone() {
        b.scopes
            .last_mut()
            .expect("no function scope")
            .insert(name, sym);
    }
    for stmt in &f.body {
        lower_stmt(&mut b, stmt)?;
    }
    // Void fallthrough: run deferred frame 0 then return.
    // (Non-void functions must return explicitly; the backend seals the rest
    // with `unreachable`, same as today.)
    use crate::frontend::tokens::builtin::BuiltinType;
    if matches!(f.return_type, Ty::Builtin(BuiltinType::Void)) {
        let needs_ret = !matches!(
            b.instrs.last(),
            Some(Instruction::Return { .. }) | Some(Instruction::Unreachable)
        );
        if needs_ret {
            let deferred: Vec<TypedStatement> = b.deferred.first().cloned().unwrap_or_default();
            for d in deferred.iter().rev() {
                lower_stmt(&mut b, d)?;
            }
            b.emit(Instruction::Return { values: vec![] });
        }
    }
    b.exit_scope();
    Ok(b.finish())
}

fn lower_deferred_from(b: &mut FunctionBuilder, from_depth: usize) -> Result<(), LowerError> {
    // Clone the frames above `from_depth` so we can recursively lower them
    // while the stack is borrowed immutably.
    let frames: Vec<Vec<TypedStatement>> = b.deferred[from_depth..].to_vec();
    for frame in frames.iter().rev() {
        for stmt in frame.iter().rev() {
            lower_stmt(b, stmt)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

fn lower_stmt(b: &mut FunctionBuilder, stmt: &TypedStatement) -> Result<(), LowerError> {
    match stmt {
        TypedStatement::Binding(binding) => {
            if matches!(binding.ty, Ty::Type) {
                return Err(LowerError::Unsupported(
                    "comptime `type` binding in runtime code".to_string(),
                ));
            }
            let sym = b.fresh_symbol(binding.name.value.clone(), binding.ty.clone());
            b.emit(Instruction::Alloca {
                sym,
                name: binding.name.value.clone(),
                ty: binding.ty.clone(),
            });
            if let Some(init) = &binding.init {
                let v = lower_expr(b, init)?;
                b.emit(Instruction::Store { dst: sym, src: v });
            }
            Ok(())
        }
        TypedStatement::Assign { name, expr } => {
            let sym = b.lookup(&name.value).ok_or_else(|| {
                LowerError::Unsupported(format!("assign to unknown variable '{}'", name))
            })?;
            let v = lower_expr(b, expr)?;
            b.emit(Instruction::Store { dst: sym, src: v });
            Ok(())
        }
        TypedStatement::Expr(e) => {
            let _ = lower_expr(b, e)?;
            Ok(())
        }
        TypedStatement::Return(ret) => {
            lower_deferred_from(b, 0)?;
            let values = match ret {
                None => vec![],
                Some(r) => {
                    let mut vs = Vec::with_capacity(r.values.len());
                    for v in &r.values {
                        vs.push(lower_expr(b, v)?);
                    }
                    vs
                }
            };
            b.emit(Instruction::Return { values });
            Ok(())
        }
        TypedStatement::If(tif) => {
            let cond = lower_expr(b, &tif.cond)?;
            let else_label = b.fresh_label();
            let merge_label = b.fresh_label();
            // if !cond goto else
            // (backend lowers IfGoto as cond_br cond then fallthrough else target;
            // to express "else" we need an explicit negated temp)
            let ntmp = b.fresh_temp();
            b.emit(Instruction::Unary {
                dst: ntmp,
                op: UnOp::Not,
                src: cond,
                ty: crate::frontend::typed_ast::Ty::Builtin(
                    crate::frontend::tokens::builtin::BuiltinType::Boolean,
                ),
            });
            b.emit(Instruction::IfGoto {
                cond: Operand::Temp(ntmp),
                target: else_label,
            });
            b.enter_scope();
            b.deferred.push(Vec::new());
            for s in &tif.then_branch {
                lower_stmt(b, s)?;
            }
            b.deferred.pop();
            b.exit_scope();
            b.emit(Instruction::Goto {
                target: merge_label,
            });
            b.emit(Instruction::LabelDef(else_label));
            if let Some(else_branch) = &tif.else_branch {
                b.enter_scope();
                b.deferred.push(Vec::new());
                for s in else_branch {
                    lower_stmt(b, s)?;
                }
                b.deferred.pop();
                b.exit_scope();
            }
            b.emit(Instruction::LabelDef(merge_label));
            Ok(())
        }
        TypedStatement::For {
            init,
            cond,
            post,
            body,
        } => {
            b.enter_scope();
            if let Some(init_stmt) = init {
                lower_stmt(b, init_stmt)?;
            }
            let cond_label = b.fresh_label();
            let body_label = b.fresh_label();
            let post_label = b.fresh_label();
            let after_label = b.fresh_label();
            b.emit(Instruction::LabelDef(cond_label));
            if let Some(c) = cond {
                let cv = lower_expr(b, c)?;
                let ntmp = b.fresh_temp();
                b.emit(Instruction::Unary {
                    dst: ntmp,
                    op: UnOp::Not,
                    src: cv,
                    ty: crate::frontend::typed_ast::Ty::Builtin(
                        crate::frontend::tokens::builtin::BuiltinType::Boolean,
                    ),
                });
                b.emit(Instruction::IfGoto {
                    cond: Operand::Temp(ntmp),
                    target: after_label,
                });
            }
            b.emit(Instruction::Goto { target: body_label });
            b.emit(Instruction::LabelDef(body_label));
            let depth = b.deferred.len();
            b.loops.push(LoopTarget {
                break_label: after_label,
                continue_label: post_label,
                deferred_depth: depth,
            });
            b.enter_scope();
            b.deferred.push(Vec::new());
            for s in body {
                lower_stmt(b, s)?;
            }
            b.deferred.pop();
            b.exit_scope();
            b.loops.pop();
            b.emit(Instruction::LabelDef(post_label));
            if let Some(post_stmt) = post {
                lower_stmt(b, post_stmt)?;
            }
            b.emit(Instruction::Goto { target: cond_label });
            b.emit(Instruction::LabelDef(after_label));
            b.exit_scope();
            Ok(())
        }
        TypedStatement::Break => {
            let target = b
                .loops
                .last()
                .ok_or_else(|| LowerError::Unsupported("`break` outside loop".to_string()))?;
            let (lbl, depth) = (target.break_label, target.deferred_depth);
            lower_deferred_from(b, depth)?;
            b.emit(Instruction::Goto { target: lbl });
            Ok(())
        }
        TypedStatement::Continue => {
            let target = b
                .loops
                .last()
                .ok_or_else(|| LowerError::Unsupported("`continue` outside loop".to_string()))?;
            let (lbl, depth) = (target.continue_label, target.deferred_depth);
            lower_deferred_from(b, depth)?;
            b.emit(Instruction::Goto { target: lbl });
            Ok(())
        }
        TypedStatement::Defer(inner) => {
            b.deferred
                .last_mut()
                .expect("deferred stack always has a frame")
                .push((**inner).clone());
            Ok(())
        }
        TypedStatement::MultiAssign { .. } => Err(LowerError::Unsupported(
            "multi-assign (tuple unpack in phase 2b)".to_string(),
        )),
        TypedStatement::MultiBinding { .. } => Err(LowerError::Unsupported(
            "multi-binding (tuple unpack in phase 2b)".to_string(),
        )),
        TypedStatement::FieldAssign { .. } => Err(LowerError::Unsupported(
            "field-assign (phase 2b: FieldStore)".to_string(),
        )),
        TypedStatement::DerefAssign { .. } => Err(LowerError::Unsupported(
            "deref-assign (phase 2b: StorePtr)".to_string(),
        )),
        TypedStatement::IndexAssign { .. } => Err(LowerError::Unsupported(
            "index-assign (phase 2b: IndexStore)".to_string(),
        )),
        TypedStatement::Switch { .. } => Err(LowerError::Unsupported(
            "switch (phase 2b: SwitchDispatch)".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Expressions (each returns the Operand holding its value)
// ---------------------------------------------------------------------------

fn lower_expr(b: &mut FunctionBuilder, expr: &TypedExpr) -> Result<Operand, LowerError> {
    match &expr.expression {
        TypedExprKind::LiteralInt { value } => Ok(Operand::ConstInt {
            value: *value,
            ty: expr.inferred_type.clone(),
        }),
        TypedExprKind::LiteralFloat { value } => Ok(Operand::ConstFloat {
            value: *value,
            ty: expr.inferred_type.clone(),
        }),
        TypedExprKind::LiteralBool(v) => Ok(Operand::ConstBool(*v)),
        TypedExprKind::LiteralString { value } => Ok(Operand::StringLit(value.clone())),
        TypedExprKind::Null => Ok(Operand::Null),
        TypedExprKind::Identifier(name) => {
            let sym = b
                .lookup(&name.value)
                .ok_or_else(|| LowerError::Unsupported(format!("unknown variable '{}'", name)))?;
            let dst = b.fresh_temp();
            b.emit(Instruction::Load { dst, src: sym });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::Binary {
            left,
            operator,
            right,
        } => {
            // Short-circuit && / || via explicit control flow + join temp.
            if matches!(operator, Operator::LogicalAnd | Operator::LogicalOr) {
                return lower_short_circuit(b, expr, left, *operator, right);
            }
            let l = lower_expr(b, left)?;
            let r = lower_expr(b, right)?;
            let op = operator_to_binop(operator).ok_or_else(|| {
                LowerError::Unsupported(format!(
                    "binary operator {:?} (assign-ops are desugared in analysis)",
                    operator
                ))
            })?;
            let dst = b.fresh_temp();
            // `ty` is the *operand* type (left's) for opcode selection
            // (`SDiv` vs `UDiv`, `SLT` vs `ULT`, ...). For comparisons the
            // result is `@bool`, but the opcode still depends on the
            // operands —mirroring `codegen_expr` which keys off
            // `left.inferred_type`.
            b.emit(Instruction::Binary {
                dst,
                op,
                lhs: l,
                rhs: r,
                ty: left.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::Call { callee, args } => {
            let mut operands = Vec::with_capacity(args.len());
            for a in args {
                operands.push(lower_expr(b, a)?);
            }
            // Void calls get no destination temp.
            use crate::frontend::tokens::builtin::BuiltinType;
            let is_void = matches!(expr.inferred_type, Ty::Builtin(BuiltinType::Void));
            let dst = if is_void { None } else { Some(b.fresh_temp()) };
            let ret = dst.map(Operand::Temp);
            b.emit(Instruction::Call {
                dst,
                callee: callee.value.clone(),
                args: operands,
                ret_ty: expr.inferred_type.clone(),
                is_variadic: false,
            });
            Ok(ret.unwrap_or(Operand::ConstBool(false)))
        }
        TypedExprKind::BuiltinCall { builtin, args } => {
            let mut operands = Vec::with_capacity(args.len());
            for a in args {
                operands.push(lower_expr(b, a)?);
            }
            let dst = b.fresh_temp();
            let ret = Operand::Temp(dst);
            b.emit(Instruction::BuiltinCall {
                dst: Some(dst),
                builtin: builtin.clone(),
                args: operands,
                ret_ty: expr.inferred_type.clone(),
            });
            Ok(ret)
        }
        TypedExprKind::Cast {
            expr: inner,
            target_type,
        } => {
            let src = lower_expr(b, inner)?;
            let dst = b.fresh_temp();
            b.emit(Instruction::Cast {
                dst,
                src,
                from: inner.inferred_type.clone(),
                target: target_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::AddressOf(inner) => {
            // Only `&x` for a named place is modeled in the core slice.
            if let TypedExprKind::Identifier(name) = &inner.expression {
                let sym = b.lookup(&name.value).ok_or_else(|| {
                    LowerError::Unsupported(format!("address-of unknown '{}'", name))
                })?;
                let dst = b.fresh_temp();
                b.emit(Instruction::AddrOf { dst, src: sym });
                Ok(Operand::Temp(dst))
            } else {
                Err(LowerError::Unsupported(
                    "address-of non-identifier (phase 2b: general lvalues)".to_string(),
                ))
            }
        }
        TypedExprKind::Deref(inner) => {
            let ptr = lower_expr(b, inner)?;
            let dst = b.fresh_temp();
            b.emit(Instruction::LoadPtr {
                dst,
                ptr,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::FieldAccess { .. } => Err(LowerError::Unsupported(
            "field access (phase 2b: FieldLoad)".to_string(),
        )),
        TypedExprKind::StructConstruct { .. } => Err(LowerError::Unsupported(
            "struct construction (phase 2b: StructConstruct)".to_string(),
        )),
        TypedExprKind::IndexAccess { .. } => Err(LowerError::Unsupported(
            "index access (phase 2b: IndexLoad)".to_string(),
        )),
        TypedExprKind::ArrayLiteral { .. } => Err(LowerError::Unsupported(
            "array literal (phase 2b: ArrayLiteral)".to_string(),
        )),
        TypedExprKind::QualifiedAccess { module, name } => Err(LowerError::Unsupported(format!(
            "qualified access {}::{} (phase 2b: module mangling)",
            module, name
        ))),
        TypedExprKind::ComptimeType(_) => Err(LowerError::Unsupported(
            "comptime type value in runtime code".to_string(),
        )),
        TypedExprKind::EnumLiteral { discriminant, .. } => {
            let dst = b.fresh_temp();
            b.emit(Instruction::EnumLiteral {
                dst,
                discriminant: *discriminant,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
        TypedExprKind::EnumVariantConstruct {
            discriminant,
            fields,
            ..
        } => {
            let mut payload = Vec::with_capacity(fields.len());
            for (fname, fexpr) in fields {
                payload.push((fname.clone(), lower_expr(b, fexpr)?));
            }
            let dst = b.fresh_temp();
            b.emit(Instruction::EnumConstruct {
                dst,
                discriminant: *discriminant,
                payload,
                ty: expr.inferred_type.clone(),
            });
            Ok(Operand::Temp(dst))
        }
    }
}

fn lower_short_circuit(
    b: &mut FunctionBuilder,
    _expr: &TypedExpr,
    left: &TypedExpr,
    op: Operator,
    right: &TypedExpr,
) -> Result<Operand, LowerError> {
    use crate::frontend::tokens::builtin::BuiltinType;
    let bool_ty = Ty::Builtin(BuiltinType::Boolean);
    let l = lower_expr(b, left)?;
    let result = b.fresh_temp();
    let rhs_label = b.fresh_label();
    let merge_label = b.fresh_label();
    let is_and = matches!(op, Operator::LogicalAnd);
    // Seed the join temp with the short-circuit value, then overwrite from RHS.
    let short_val = Operand::ConstBool(!is_and);
    b.emit(Instruction::Copy {
        dst: result,
        src: short_val,
    });
    // if (is_and ? !l : l) goto merge  <=>  skip RHS when decided
    if is_and {
        let ntmp = b.fresh_temp();
        b.emit(Instruction::Unary {
            dst: ntmp,
            op: UnOp::Not,
            src: l,
            ty: bool_ty.clone(),
        });
        b.emit(Instruction::IfGoto {
            cond: Operand::Temp(ntmp),
            target: merge_label,
        });
    } else {
        b.emit(Instruction::IfGoto {
            cond: l,
            target: merge_label,
        });
    }
    b.emit(Instruction::Goto { target: rhs_label });
    b.emit(Instruction::LabelDef(rhs_label));
    let r = lower_expr(b, right)?;
    b.emit(Instruction::Copy {
        dst: result,
        src: r,
    });
    b.emit(Instruction::LabelDef(merge_label));
    Ok(Operand::Temp(result))
}

// ---------------------------------------------------------------------------
// Pretty printing (used by tests and `--emit=ir` later)
// ---------------------------------------------------------------------------

impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%t{}", self.0)
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "L{}", self.0)
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%s{}", self.0)
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temp(t) => write!(f, "{}", t),
            Self::Symbol(s) => write!(f, "{}", s),
            Self::ConstInt { value, ty } => write!(f, "{}:{}", value, ty),
            Self::ConstFloat { value, ty } => write!(f, "{}:{}", value, ty),
            Self::ConstBool(v) => write!(f, "{}", v),
            Self::StringLit(s) => write!(f, "\"{}\"", s),
            Self::Null => write!(f, "null"),
        }
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Alloca { sym, name, ty } => write!(f, "{} = alloca {} : {}", sym, name, ty),
            Self::Store { dst, src } => write!(f, "store {} <- {}", dst, src),
            Self::Load { dst, src } => write!(f, "{} = load {}", dst, src),
            Self::Copy { dst, src } => write!(f, "{} = copy {}", dst, src),
            Self::Binary {
                dst,
                op,
                lhs,
                rhs,
                ty,
            } => {
                write!(f, "{} = {} {}, {} : {}", dst, op, lhs, rhs, ty)
            }
            Self::Unary { dst, op, src, ty } => {
                let s = match op {
                    UnOp::Neg => "neg",
                    UnOp::Not => "not",
                };
                write!(f, "{} = {} {} : {}", dst, s, src, ty)
            }
            Self::AddrOf { dst, src } => write!(f, "{} = addr_of {}", dst, src),
            Self::LoadPtr { dst, ptr, ty } => write!(f, "{} = load_ptr {} : {}", dst, ptr, ty),
            Self::StorePtr { ptr, src } => write!(f, "store_ptr {} <- {}", ptr, src),
            Self::Cast {
                dst, src, target, ..
            } => write!(f, "{} = cast {} to {}", dst, src, target),
            Self::Call {
                dst, callee, args, ..
            } => {
                let arg_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                match dst {
                    Some(d) => write!(f, "{} = call {}({})", d, callee, arg_str),
                    None => write!(f, "call {}({})", callee, arg_str),
                }
            }
            Self::BuiltinCall {
                dst, builtin, args, ..
            } => {
                let arg_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                match dst {
                    Some(d) => write!(f, "{} = builtin @{}({})", d, builtin.name(), arg_str),
                    None => write!(f, "builtin @{}({})", builtin.name(), arg_str),
                }
            }
            Self::StructConstruct {
                dst,
                type_name,
                fields,
                ..
            } => {
                let fs = fields
                    .iter()
                    .map(|(n, o)| format!("{}: {}", n, o))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} = struct {} {{ {} }}", dst, type_name, fs)
            }
            Self::FieldLoad {
                dst,
                base,
                field_index,
                ..
            } => {
                write!(f, "{} = field_load {}.{} ", dst, base, field_index)
            }
            Self::FieldStore {
                base,
                field_index,
                src,
            } => {
                write!(f, "field_store {}.{} <- {}", base, field_index, src)
            }
            Self::ArrayLiteral { dst, elems, .. } => {
                let es = elems
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} = array [{}]", dst, es)
            }
            Self::IndexLoad {
                dst, base, index, ..
            } => {
                write!(f, "{} = index_load {}[{}]", dst, base, index)
            }
            Self::IndexStore { base, index, src } => {
                write!(f, "index_store {}[{}] <- {}", base, index, src)
            }
            Self::EnumLiteral {
                dst, discriminant, ..
            } => {
                write!(f, "{} = enum_tag {}", dst, discriminant)
            }
            Self::EnumConstruct {
                dst,
                discriminant,
                payload,
                ..
            } => {
                let ps = payload
                    .iter()
                    .map(|(n, o)| format!("{}: {}", n, o))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{} = enum_construct {} {{ {} }}", dst, discriminant, ps)
            }
            Self::SwitchDispatch {
                subject,
                cases,
                default,
            } => {
                let cs = cases
                    .iter()
                    .map(|(d, l)| format!("{} -> {}", d, l))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "switch {} [{}] default {}", subject, cs, default)
            }
            Self::IfGoto { cond, target } => write!(f, "if {} goto {}", cond, target),
            Self::Goto { target } => write!(f, "goto {}", target),
            Self::LabelDef(l) => write!(f, "{}:", l),
            Self::Return { values } => {
                let vs = values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "return {}", vs)
            }
            Self::Unreachable => write!(f, "unreachable"),
        }
    }
}

impl fmt::Display for IrFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = self
            .params
            .iter()
            .map(|(s, n, t)| format!("{} {}: {}", s, n, t))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(f, "fn {}({}) -> {} {{", self.name, params, self.return_ty)?;
        for (s, n, t) in &self.symbols {
            writeln!(f, "  ; sym {} {} : {}", s, n, t)?;
        }
        for block in &self.blocks {
            writeln!(f, "  {}:", block.label)?;
            for instr in &block.instrs {
                // Skip duplicate label defs (already printed as block header)
                if let Instruction::LabelDef(l) = instr
                    && *l == block.label
                {
                    continue;
                }
                writeln!(f, "    {}", instr)?;
            }
        }
        writeln!(f, "}}")
    }
}

impl fmt::Display for IrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for func in &self.functions {
            writeln!(f, "{}", func)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests: the IR contract (pretty-print + error cases)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use std::path::PathBuf;

    fn lower_src(src: &str) -> Result<IrProgram, String> {
        let tokens: Vec<_> = Lexer::new(src).collect();
        let path = PathBuf::from("test.fib");
        let mut parser = Parser::new(tokens.into_iter(), &path, src.to_string());
        let ast = parser.parse().map_err(|e| e.to_string())?;
        let typed = crate::frontend::analyze::analyze(ast, &Default::default())
            .map_err(|e| e.to_string())?;
        lower_typed_program(typed).map_err(|e| e.to_string())
    }

    #[test]
    fn ir_straight_line_binding_and_return() {
        let prog = lower_src("fn main() @int { x := 1\ny := 2\nreturn x + y }")
            .expect("lower straight-line");
        assert_eq!(prog.functions.len(), 1);
        let text = prog.to_string();
        assert!(
            text.contains("alloca x"),
            "expected alloca x, got:\n{}",
            text
        );
        assert!(
            text.contains("alloca y"),
            "expected alloca y, got:\n{}",
            text
        );
        assert!(text.contains("add"), "expected add, got:\n{}", text);
        assert!(text.contains("return"), "expected return, got:\n{}", text);
    }

    #[test]
    fn ir_if_and_for_emit_labels_and_gotos() {
        let prog = lower_src(
            "fn main() @int { x := 0\nif x == 0 { x = 1 } else { x = 2 }\nfor (i: @int = 0; i < 10; i = i + 1) { x = x + i }\nreturn x }",
        )
        .expect("lower if+for");
        let text = prog.to_string();
        assert!(text.contains("if "), "expected IfGoto, got:\n{}", text);
        assert!(text.contains("goto "), "expected Goto, got:\n{}", text);
        assert!(text.contains("return"), "expected return, got:\n{}", text);
    }

    #[test]
    fn ir_short_circuit_uses_join_temp() {
        let prog = lower_src("fn main() @bool { a := true\nb := false\nreturn a && b }")
            .expect("lower &&");
        let text = prog.to_string();
        // Join pattern: seed copy + rhs copy + merge label.
        assert!(text.contains("copy"), "expected join copy, got:\n{}", text);
        assert!(text.contains("if "), "expected IfGoto, got:\n{}", text);
    }

    #[test]
    fn ir_defer_runs_before_return() {
        let prog =
            lower_src("fn main() @int { x := 0\ndefer x = 1\nreturn x }").expect("lower defer");
        let text = prog.to_string();
        // Deferred store must appear before the return.
        let store_pos = text.find("store ").expect("expected store");
        let ret_pos = text.find("return").expect("expected return");
        assert!(
            store_pos < ret_pos,
            "defer store should precede return:\n{}",
            text
        );
    }

    #[test]
    fn ir_break_continue_become_gotos() {
        let prog = lower_src(
            "fn main() @int { x := 0\nfor (i: @int = 0; i < 10; i = i + 1) { if i == 2 { continue }\nif i == 5 { break }\nx = x + i }\nreturn x }",
        )
        .expect("lower break/continue");
        let text = prog.to_string();
        assert!(text.contains("goto "), "expected goto lowering:\n{}", text);
    }

    #[test]
    fn ir_symbols_survive_shadowing() {
        let prog = lower_src("fn main() @int { x := 1\nif true { x := 2 }\nreturn x }")
            .expect("lower shadowing");
        let func = &prog.functions[0];
        // Two distinct slots for the two `x` bindings.
        assert!(
            func.symbols.iter().filter(|(_, n, _)| n == "x").count() == 2,
            "expected 2 symbols for shadowed x, got {:?}",
            func.symbols
        );
    }

    #[test]
    fn ir_extern_has_no_body() {
        let prog = lower_src("extern fn puts(s: @string) @int\nfn main() @int { return 0 }")
            .expect("lower extern");
        let ext = prog
            .functions
            .iter()
            .find(|f| f.name == "puts")
            .expect("extern fn present");
        assert!(ext.is_extern);
        assert!(ext.blocks.iter().all(|b| b.instrs.is_empty()));
    }

    #[test]
    fn ir_switch_is_explicitly_unsupported() {
        let err = lower_src(
            "type Color enum { Red, Green }\nfn main() @int { c: Color = Color.Red\nswitch (c) { when .Red { return 1 }\nwhen else { return 0 } } }",
        )
        .expect_err("switch should be phase 2b");
        assert!(err.contains("switch"), "unexpected error: {}", err);
    }

    #[test]
    fn ir_struct_is_explicitly_unsupported() {
        let err = lower_src(
            "type Point struct { x: @int, y: @int }\nfn main() @int { p := Point { x: 1, y: 2 }\nreturn p.x }",
        )
        .expect_err("struct should be phase 2b");
        assert!(
            err.contains("struct") || err.contains("field"),
            "unexpected error: {}",
            err
        );
    }
}
