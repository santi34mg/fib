pub struct Label(pub u32);
pub struct Temp(pub u32);

pub enum Instruction<'a> {
    // x = y op z
    BinaryOpAssign {
        destination: Address,
        first_operand: Address,
        operation: Operation,
        second_operand: Address,
    },
    // x = op y
    UnaryOpAssign {
        destination: Address,
        operation: Operation,
        operand: Address,
    },
    // goto L
    GotoInstruction(&'a Instruction<'a>),
    // if x goto L
}

pub enum Operation {}

pub enum Address {
    Label(Label),
    Temp(Temp),
}

pub enum Value {}
