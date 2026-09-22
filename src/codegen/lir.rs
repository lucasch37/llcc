use crate::{codegen::register::ImmediateValue, semantic::types::Type};

use super::register::Operand;

#[derive(Debug, Clone)]
pub struct LabelId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    SignExtend,
    ZeroExtend,
    Truncate,

    IntToFloat,
    IntToDouble,

    FloatToInt,
    FloatToDouble,

    DoubleToInt,
    DoubleToFloat,
}

#[derive(Debug, Clone)]
pub enum Lir {
    Global {
        name: String,
        typ: Type,
        initializer: Option<ImmediateValue>,
    },

    FunctionStart {
        name: String,
        stack_size: usize,
    },

    FunctionEnd,

    Label(LabelId),

    Jmp(LabelId),

    Mov {
        dst: Operand,
        src: Operand,
    },

    Add {
        dst: Operand,
        src: Operand,
    },

    Sub {
        dst: Operand,
        src: Operand,
    },

    Mul {
        dst: Operand,
        src: Operand,
    },

    Div {
        divisor: Operand,
        typ: Type,
        signed: bool,
    },

    Cmp {
        lhs: Operand,
        rhs: Operand,
    },

    Lea {
        dst: Operand,
        src: Operand,
    },

    Cast {
        kind: CastKind,
        dst: Operand,
        src: Operand,
    },

    Call {
        target: String,
    },

    Ret,
}
