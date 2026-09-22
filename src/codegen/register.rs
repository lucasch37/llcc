use std::collections::HashMap;

use crate::semantic::types::{Primitive, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VRegId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct VirtualReg {
    pub id: VRegId,
    pub typ: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackSlot {
    pub offset: usize,
    pub typ: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: String,
    pub typ: Type,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImmediateValue {
    Int(i32),
    Float(f32),
    Double(f64),
    Char(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalReg {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,

    Xmm0,
    Xmm1,
    Xmm2,
    Xmm3,
    Xmm4,
    Xmm5,
    Xmm6,
    Xmm7,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Virtual(VirtualReg),

    Stack(StackSlot),

    Global(Global),

    Immediate { value: ImmediateValue, typ: Type },

    Physical { reg: PhysicalReg, typ: Type },

    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Allocation {
    Register(PhysicalReg),
    Spill(StackSlot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterClass {
    Gpr,
    Xmm,
}

impl VirtualReg {
    pub fn class(&self) -> RegisterClass {
        register_class(&self.typ)
    }
}

impl Operand {
    pub fn typ(&self) -> Option<&Type> {
        match self {
            Operand::Virtual(reg) => Some(&reg.typ),
            Operand::Stack(slot) => Some(&slot.typ),
            Operand::Global(global) => Some(&global.typ),
            Operand::Immediate { typ, .. } => Some(typ),
            Operand::Physical { typ, .. } => Some(typ),
            Operand::Void => None,
        }
    }

    pub fn vreg_id(&self) -> Option<VRegId> {
        match self {
            Operand::Virtual(reg) => Some(reg.id),
            _ => None,
        }
    }
}

pub fn register_class(typ: &Type) -> RegisterClass {
    match typ {
        Type::Primitive(Primitive::Float) | Type::Primitive(Primitive::Double) => {
            RegisterClass::Xmm
        }

        _ => RegisterClass::Gpr,
    }
}

pub fn resolve_allocation<'a>(
    operand: &'a Operand,
    allocations: &'a HashMap<VRegId, Allocation>,
) -> Option<&'a Allocation> {
    let id = operand.vreg_id()?;
    allocations.get(&id)
}
