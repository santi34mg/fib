use std::collections::HashMap;

use super::{BasicBlock, Instruction, IrFunction, Label, SymbolId, Temp};
use crate::frontend::typed_ast::{Ty, TypedStatement};
// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub(super) struct LoopTarget {
    pub(super) break_label: Label,
    pub(super) continue_label: Label,
    pub(super) deferred_depth: usize,
}

pub(super) struct FunctionBuilder {
    pub(super) name: String,
    pub(super) params: Vec<(SymbolId, String, Ty)>,
    pub(super) return_ty: Ty,
    pub(super) is_extern: bool,
    pub(super) is_variadic: bool,
    pub(super) next_temp: u32,
    pub(super) next_label: u32,
    pub(super) next_sym: u32,
    pub(super) symbols: Vec<(SymbolId, String, Ty)>,
    pub(super) scopes: Vec<HashMap<String, SymbolId>>,
    pub(super) instrs: Vec<Instruction>,
    pub(super) deferred: Vec<Vec<TypedStatement>>,
    pub(super) loops: Vec<LoopTarget>,
}

impl FunctionBuilder {
    pub(super) fn new(name: String, return_ty: Ty, is_extern: bool, is_variadic: bool) -> Self {
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

    pub(super) fn fresh_temp(&mut self) -> Temp {
        let t = Temp(self.next_temp);
        self.next_temp += 1;
        t
    }

    pub(super) fn fresh_label(&mut self) -> Label {
        let l = Label(self.next_label);
        self.next_label += 1;
        l
    }

    pub(super) fn fresh_symbol(&mut self, name: String, ty: Ty) -> SymbolId {
        let s = SymbolId(self.next_sym);
        self.next_sym += 1;
        self.symbols.push((s, name.clone(), ty));
        self.scopes
            .last_mut()
            .expect("FunctionBuilder has no scope")
            .insert(name, s);
        s
    }

    pub(super) fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }

    pub(super) fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(super) fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn emit(&mut self, instr: Instruction) {
        self.instrs.push(instr);
    }

    pub(super) fn finish(mut self) -> IrFunction {
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
