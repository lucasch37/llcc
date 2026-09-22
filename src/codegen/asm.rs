use std::collections::HashMap;

use crate::semantic::types::Type;

use super::lir::{CastKind, Condition, Lir};
use super::register::{Allocation, ImmediateValue, Operand, PhysicalReg, VRegId};

pub struct Asm<'a> {
    allocations: &'a HashMap<VRegId, Allocation>,
    output: String,
}

impl<'a> Asm<'a> {
    pub fn new(allocations: &'a HashMap<VRegId, Allocation>) -> Self {
        Self {
            allocations,
            output: String::new(),
        }
    }

    pub fn emit_all(mut self, instructions: &[Lir]) -> String {
        self.line(".intel_syntax noprefix");

        for instruction in instructions {
            self.emit_instruction(instruction);
        }

        self.output
    }

    fn line(&mut self, line: impl AsRef<str>) {
        self.output.push_str(line.as_ref());
        self.output.push('\n');
    }

    fn emit_instruction(&mut self, instruction: &Lir) {
        match instruction {
            Lir::Global {
                name,
                typ,
                initializer,
            } => {
                if let Some(initializer) = initializer {
                    self.line(".data");
                } else {
                    self.line(".bss")
                }

                self.line(format!(".globl _{name}"));
                self.line(format!("_{name}:"));

                if let Some(initializer) = initializer {
                    match initializer {
                        ImmediateValue::Int(value) => {
                            self.line(format!("    .long {value}"));
                        }

                        ImmediateValue::Float(value) => {
                            self.line(format!("    .long {value}"));
                        }

                        ImmediateValue::Double(value) => {
                            self.line(format!("    .quad {value}"));
                        }

                        ImmediateValue::Char(value) => {
                            self.line(format!("    .byte {value}"));
                        }
                    }
                } else {
                    let size = typ.size();
                    self.line(format!("    .zero {size}"));
                }
            }

            Lir::FunctionStart { name, stack_size } => {
                self.line(".text");
                self.line(format!(".globl _{name}"));
                self.line(format!("_{name}:"));
                self.line("    push rbp");
                self.line("    mov rbp, rsp");

                if *stack_size > 0 {
                    let aligned = align16(*stack_size);
                    self.line(format!("    sub rsp, {aligned}"));
                }
            }

            Lir::FunctionEnd => {
                self.line("    mov rsp, rbp");
                self.line("    pop rbp");
                self.line("    ret");
            }

            Lir::Label(id) => {
                let id = id.0;
                self.line(format!(".L{id}:"));
            }

            Lir::Jmp(id) => {
                let id = id.0;
                self.line(format!("    jmp .L{id}"));
            }

            Lir::Mov { src, dst } => {
                let typ = dst
                    .typ()
                    .or_else(|| src.typ())
                    .expect("mov needs typed operands");

                self.line(format!(
                    "    mov {}, {}",
                    self.operand_typed(dst, typ),
                    self.operand_typed(src, typ),
                ));
            }

            Lir::Add { src, dst } => {
                let typ = dst.typ().expect("add dst must be typed");

                self.line(format!(
                    "    add {}, {}",
                    self.operand_typed(dst, typ),
                    self.operand_typed(src, typ),
                ));
            }

            Lir::Sub { src, dst } => {
                let typ = dst.typ().expect("sub dst must be typed");

                self.line(format!(
                    "    sub {}, {}",
                    self.operand_typed(dst, typ),
                    self.operand_typed(src, typ),
                ));
            }

            Lir::Mul { src, dst } => {
                let typ = dst.typ().expect("mul dst must be typed");

                self.line(format!(
                    "    imul {}, {}",
                    self.operand_typed(dst, typ),
                    self.operand_typed(src, typ),
                ));
            }

            Lir::Div {
                divisor,
                typ,
                signed,
            } => {
                if *signed {
                    if typ.size() <= 4 {
                        self.line("    cdq");
                    } else {
                        self.line("    cqo");
                    }

                    self.line(format!("    idiv {}", self.operand_typed(divisor, typ),));
                } else {
                    match typ.size() {
                        1 | 2 | 4 => self.line("    xor edx, edx"),
                        8 => self.line("    xor rdx, rdx"),
                        size => unreachable!("unsupported division size {size}"),
                    }

                    self.line(format!("    div {}", self.operand_typed(divisor, typ),));
                }
            }

            Lir::Cmp { lhs, rhs } => {
                let typ = lhs
                    .typ()
                    .or_else(|| rhs.typ())
                    .expect("cmp needs typed operands");

                self.line(format!(
                    "    cmp {}, {}",
                    self.operand_typed(lhs, typ),
                    self.operand_typed(rhs, typ),
                ));
            }

            Lir::Lea { src, dst } => {
                self.line(format!(
                    "    lea {}, {}",
                    self.operand(dst),
                    self.address_operand(src),
                ));
            }

            Lir::Cast { kind, src, dst } => {
                self.emit_cast(*kind, src, dst);
            }

            Lir::Call { target } => {
                self.line(format!("    call _{target}"));
            }

            Lir::Ret => {
                self.line("    ret");
            }
        }
    }

    fn emit_cast(&mut self, kind: CastKind, src: &Operand, dst: &Operand) {
        match kind {
            CastKind::SignExtend => {
                let from = src.typ().expect("cast src type");
                let to = dst.typ().expect("cast dst type");

                let instruction = match (from.size(), to.size()) {
                    (1, 2) | (1, 4) | (1, 8) | (2, 4) | (2, 8) => "movsx",
                    (4, 8) => "movsxd",
                    _ => unreachable!("invalid sign extension {} -> {}", from.size(), to.size()),
                };

                self.line(format!(
                    "    {instruction} {}, {}",
                    self.operand_with_size(dst, to.size()),
                    self.operand_with_size(src, from.size()),
                ));
            }

            CastKind::ZeroExtend => {
                let from = src.typ().expect("cast src type");
                let to = dst.typ().expect("cast dst type");

                match (from.size(), to.size()) {
                    (1, 2) | (1, 4) | (1, 8) | (2, 4) | (2, 8) => {
                        self.line(format!(
                            "    movzx {}, {}",
                            self.operand_with_size(dst, to.size()),
                            self.operand_with_size(src, from.size()),
                        ));
                    }

                    (4, 8) => {
                        self.line(format!(
                            "    mov {}, {}",
                            self.operand_with_size(dst, 4),
                            self.operand_with_size(src, 4),
                        ));
                    }

                    _ => unreachable!("invalid zero extension {} -> {}", from.size(), to.size()),
                }
            }

            CastKind::Truncate => {
                let to = dst.typ().expect("cast dst type");

                self.line(format!(
                    "    mov {}, {}",
                    self.operand_with_size(dst, to.size()),
                    self.operand_with_size(src, to.size()),
                ));
            }

            CastKind::IntToFloat => {
                let src_type = src.typ().expect("cast src type");

                self.line(format!(
                    "    cvtsi2ss {}, {}",
                    self.operand(dst),
                    self.operand_typed(src, src_type),
                ));
            }

            CastKind::IntToDouble => {
                let src_type = src.typ().expect("cast src type");

                self.line(format!(
                    "    cvtsi2sd {}, {}",
                    self.operand(dst),
                    self.operand_typed(src, src_type),
                ));
            }

            CastKind::FloatToInt => {
                let dst_type = dst.typ().expect("cast dst type");

                self.line(format!(
                    "    cvttss2si {}, {}",
                    self.operand_typed(dst, dst_type),
                    self.operand(src),
                ));
            }

            CastKind::DoubleToInt => {
                let dst_type = dst.typ().expect("cast dst type");

                self.line(format!(
                    "    cvttsd2si {}, {}",
                    self.operand_typed(dst, dst_type),
                    self.operand(src),
                ));
            }

            CastKind::FloatToDouble => {
                self.line(format!(
                    "    cvtss2sd {}, {}",
                    self.operand(dst),
                    self.operand(src),
                ));
            }

            CastKind::DoubleToFloat => {
                self.line(format!(
                    "    cvtsd2ss {}, {}",
                    self.operand(dst),
                    self.operand(src),
                ));
            }
        }
    }

    fn operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Virtual(reg) => match self.allocations.get(&reg.id) {
                Some(Allocation::Register(physical)) => {
                    physical_name(*physical, &reg.typ).to_string()
                }

                Some(Allocation::Spill(slot)) => {
                    format!("[rbp - {}]", slot.offset)
                }

                None => unreachable!("virtual register {:?} should have an allocation", reg.id),
            },

            Operand::Stack(slot) => format!("[rbp - {}]", slot.offset),

            Operand::Global(global) => {
                format!("[rip + _{}]", global.name)
            }

            Operand::Immediate { value, .. } => match value {
                ImmediateValue::Int(value) => value.to_string(),
                ImmediateValue::Float(value) => value.to_string(),
                ImmediateValue::Double(value) => value.to_string(),
                ImmediateValue::Char(value) => (*value as i64).to_string(),
            },

            Operand::Physical { reg, typ } => physical_name(*reg, typ).to_string(),

            Operand::Void => unreachable!("void cannot be emitted as an operand"),
        }
    }

    fn operand_typed(&self, operand: &Operand, typ: &Type) -> String {
        self.operand_with_size(operand, typ.size())
    }

    fn operand_with_size(&self, operand: &Operand, size: usize) -> String {
        match operand {
            Operand::Virtual(reg) => match self.allocations.get(&reg.id) {
                Some(Allocation::Register(physical)) => {
                    physical_name_for_size(*physical, size).to_string()
                }

                Some(Allocation::Spill(slot)) => {
                    format!("{} PTR [rbp - {}]", ptr_size(size), slot.offset)
                }

                None => unreachable!("virtual register {:?} should have an allocation", reg.id),
            },

            Operand::Stack(slot) => {
                format!("{} PTR [rbp - {}]", ptr_size(size), slot.offset)
            }

            Operand::Global(global) => {
                format!("{} PTR [rip + _{}]", ptr_size(size), global.name)
            }

            Operand::Immediate { value, .. } => match value {
                ImmediateValue::Int(value) => value.to_string(),
                ImmediateValue::Float(value) => value.to_string(),
                ImmediateValue::Double(value) => value.to_string(),
                ImmediateValue::Char(value) => (*value as i64).to_string(),
            },

            Operand::Physical { reg, .. } => physical_name_for_size(*reg, size).to_string(),

            Operand::Void => unreachable!("void cannot be emitted as an operand"),
        }
    }

    fn address_operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Virtual(reg) => match self.allocations.get(&reg.id) {
                Some(Allocation::Spill(slot)) => {
                    format!("[rbp - {}]", slot.offset)
                }

                Some(Allocation::Register(_)) => {
                    unreachable!("cannot use a register value as an LEA memory operand")
                }

                None => unreachable!("virtual register {:?} should have an allocation", reg.id),
            },

            Operand::Stack(slot) => format!("[rbp - {}]", slot.offset),

            Operand::Global(global) => {
                format!("[rip + _{}]", global.name)
            }

            _ => unreachable!("operand cannot be used as an LEA address"),
        }
    }
}

fn align16(value: usize) -> usize {
    (value + 15) & !15
}

fn ptr_size(size: usize) -> &'static str {
    match size {
        1 => "BYTE",
        2 => "WORD",
        4 => "DWORD",
        8 => "QWORD",
        size => unreachable!("unsupported memory operand size {size}"),
    }
}

fn physical_name(reg: PhysicalReg, typ: &Type) -> &'static str {
    physical_name_for_size(reg, typ.size())
}

fn physical_name_for_size(reg: PhysicalReg, size: usize) -> &'static str {
    match reg {
        PhysicalReg::Rax => gpr_name(size, "rax", "eax", "ax", "al"),
        PhysicalReg::Rbx => gpr_name(size, "rbx", "ebx", "bx", "bl"),
        PhysicalReg::Rcx => gpr_name(size, "rcx", "ecx", "cx", "cl"),
        PhysicalReg::Rdx => gpr_name(size, "rdx", "edx", "dx", "dl"),
        PhysicalReg::Rsi => gpr_name(size, "rsi", "esi", "si", "sil"),
        PhysicalReg::Rdi => gpr_name(size, "rdi", "edi", "di", "dil"),

        PhysicalReg::R8 => gpr_name(size, "r8", "r8d", "r8w", "r8b"),
        PhysicalReg::R9 => gpr_name(size, "r9", "r9d", "r9w", "r9b"),
        PhysicalReg::R10 => gpr_name(size, "r10", "r10d", "r10w", "r10b"),
        PhysicalReg::R11 => gpr_name(size, "r11", "r11d", "r11w", "r11b"),
        PhysicalReg::R12 => gpr_name(size, "r12", "r12d", "r12w", "r12b"),
        PhysicalReg::R13 => gpr_name(size, "r13", "r13d", "r13w", "r13b"),
        PhysicalReg::R14 => gpr_name(size, "r14", "r14d", "r14w", "r14b"),
        PhysicalReg::R15 => gpr_name(size, "r15", "r15d", "r15w", "r15b"),

        PhysicalReg::Xmm0 => "xmm0",
        PhysicalReg::Xmm1 => "xmm1",
        PhysicalReg::Xmm2 => "xmm2",
        PhysicalReg::Xmm3 => "xmm3",
        PhysicalReg::Xmm4 => "xmm4",
        PhysicalReg::Xmm5 => "xmm5",
        PhysicalReg::Xmm6 => "xmm6",
        PhysicalReg::Xmm7 => "xmm7",
    }
}

fn gpr_name(
    size: usize,
    qword: &'static str,
    dword: &'static str,
    word: &'static str,
    byte: &'static str,
) -> &'static str {
    match size {
        1 => byte,
        2 => word,
        4 => dword,
        8 => qword,
        size => unreachable!("unsupported GPR type size {size}"),
    }
}
