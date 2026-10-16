use std::collections::HashMap;

use super::{BasicBlock, Instruction, Label, NameId, Temp};
use crate::frontend::typed_ast::{ty::Ty, typed_statement::TypedStatement};
// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub(super) struct LoopTarget {
    pub(super) break_label: Label,
    pub(super) continue_label: Label,
    pub(super) deferred_depth: usize,
}

pub(super) struct BasicBlockBuilder<'i, 't> {
    pub(super) params: Vec<(NameId, &'t str, Ty<'t>)>,
    pub(super) return_ty: Option<Ty<'t>>,
    pub(super) is_extern: Option<bool>,
    pub(super) is_variadic: Option<bool>,
    pub(super) next_temp: u32,
    pub(super) next_label: u32,
    pub(super) next_sym: u64,
    pub(super) symbols: Vec<(NameId, &'t str, Ty<'t>)>,
    pub(super) scopes: Vec<HashMap<&'t str, NameId>>,
    pub(super) instrs: Vec<Instruction<'i>>,
    pub(super) deferred: Vec<Vec<TypedStatement<'t>>>,
    pub(super) loops: Vec<LoopTarget>,
}

impl<'i, 't> BasicBlockBuilder<'i, 't> {
    pub(super) fn new() -> Self {
        Self {
            params: Vec::new(),
            return_ty: None,
            is_extern: None,
            is_variadic: None,
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

    pub(super) fn fresh_symbol(&mut self, name: String, ty: Ty) -> NameId {
        let s = NameId(self.next_sym);
        self.next_sym += 1;
        self.symbols.push((s, name.clone(), ty));
        // The builder always carries at least the outer frame; degrade
        // gracefully (fresh outer frame) instead of panicking if not.
        if self.scopes.is_empty() {
            debug_assert!(false, "FunctionBuilder used with no scope");
            self.scopes.push(HashMap::new());
        }
        if let Some(frame) = self.scopes.last_mut() {
            frame.insert(name, s);
        }
        s
    }

    pub(super) fn lookup(&self, name: &str) -> Option<NameId> {
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

    pub(super) fn emit(&mut self, instr: Instruction<'i>) {
        self.instrs.push(instr);
    }

    pub(super) fn emitted_any(&self) -> bool {
        !self.instrs.is_empty()
    }

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

    pub(super) fn finish(mut self) -> Vec<BasicBlock<'i>> {
        // TODO: this does work that is probably not needed

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
                let term = Self::is_terminator(&instr);
                cur_instrs.push(instr);
                if term {
                    let label = cur_label.take().unwrap_or_else(|| {
                        let l = Label(self.next_label);
                        self.next_label += 1;
                        l
                    });
                    blocks.push(BasicBlock {
                        label,
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
        blocks
    }
}
