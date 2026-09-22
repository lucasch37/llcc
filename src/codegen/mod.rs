mod allocation;
mod asm;
mod lir;
mod register;

use std::collections::HashMap;

use crate::codegen::lir::LabelId;
use crate::codegen::register::Global;
use crate::semantic::env::SymbolId;
use crate::semantic::tir;
use crate::semantic::types::{Primitive, Type};

use allocation::{LiveInterval, RegisterAllocator};
use lir::{CastKind, Lir};
use register::{Allocation, ImmediateValue, Operand, PhysicalReg, StackSlot, VRegId, VirtualReg};

pub struct Codegen {
    unit: tir::TranslationUnit,
    instructions: Vec<Lir>,

    next_vreg: usize,
    next_label: usize,
    instruction_index: usize,

    live_starts: HashMap<VRegId, usize>,
    live_intervals: HashMap<VRegId, LiveInterval>,
    allocations: HashMap<VRegId, Allocation>,

    globals: HashMap<SymbolId, Operand>,
    locals: HashMap<SymbolId, Operand>,

    stack_size: usize,
}

struct GlobalObject {
    symbol_id: SymbolId,
    initializer: Option<tir::Initializer>,
}

impl Codegen {
    pub fn new(unit: tir::TranslationUnit) -> Self {
        Self {
            unit,
            instructions: Vec::new(),

            next_vreg: 0,
            next_label: 0,
            instruction_index: 0,

            live_starts: HashMap::new(),
            live_intervals: HashMap::new(),
            allocations: HashMap::new(),

            globals: HashMap::new(),
            locals: HashMap::new(),

            stack_size: 0,
        }
    }

    pub fn generate(mut self) -> String {
        self.lower_translation_unit();
        asm::Asm::new(&self.allocations).emit_all(&self.instructions)
    }

    fn write_lir(&mut self, instruction: Lir) {
        self.instructions.push(instruction);
        self.instruction_index += 1;
    }

    fn temp(&mut self, typ: Type) -> Operand {
        let id = VRegId(self.next_vreg);
        self.next_vreg += 1;

        self.live_starts.insert(id, self.instruction_index);

        Operand::Virtual(VirtualReg { id, typ })
    }

    fn free(&mut self, operand: &Operand) {
        let Operand::Virtual(reg) = operand else {
            return;
        };

        let Some(start) = self.live_starts.remove(&reg.id) else {
            return;
        };

        self.live_intervals.insert(
            reg.id,
            LiveInterval::new(start, self.instruction_index, reg.typ.clone()),
        );
    }

    fn make_temp(&mut self, value: Operand) -> Operand {
        if matches!(value, Operand::Virtual(_)) {
            return value;
        }

        let typ = value
            .typ()
            .expect("void cannot be moved into a temporary")
            .clone();

        let result = self.temp(typ);

        self.write_lir(Lir::Mov {
            dst: result.clone(),
            src: value,
        });

        result
    }

    fn stack_slot(&mut self, typ: Type) -> Operand {
        let size = typ.size();

        self.stack_size += size;

        let align = size.max(1);
        self.stack_size = (self.stack_size + align - 1) / align * align;

        Operand::Stack(StackSlot {
            offset: self.stack_size,
            typ,
        })
    }

    pub fn get_var(&self, symbol: &SymbolId) -> Operand {
        if let Some(global) = self.globals.get(symbol) {
            return global.clone();
        }
        if let Some(local) = self.locals.get(symbol) {
            return local.clone();
        }

        unreachable!(
            "symbol should be either a global or a local variable: {:?}",
            symbol
        )
    }

    pub fn new_label(&mut self) -> LabelId {
        let id = LabelId(self.next_label);
        self.next_label += 1;
        id
    }

    fn lower_translation_unit(&mut self) {
        self.lower_globals(self.unit.external_declarations.clone());

        for decl in self.unit.external_declarations.clone() {
            match decl {
                tir::ExternalDeclaration::FunctionDef(func) => {
                    self.lower_function_def(&func);
                }
                tir::ExternalDeclaration::Declaration(_) => {}
            }
        }
    }

    fn lower_globals(&mut self, decls: Vec<tir::ExternalDeclaration>) {
        let mut globals = HashMap::<SymbolId, GlobalObject>::new();

        for decl in decls {
            let tir::ExternalDeclaration::Declaration(decl) = decl else {
                continue;
            };

            for declarator in decl.declarators {
                let symbol_id = declarator.symbol_id;

                let typ = declarator.qtype.typ.clone();
                if typ.is_function() {
                    continue;
                }

                let global = globals.entry(symbol_id).or_insert(GlobalObject {
                    symbol_id,
                    initializer: None,
                });

                if let Some(initializer) = declarator.initializer {
                    global.initializer = Some(initializer);
                }
            }
        }

        for (symbol_id, global) in globals {
            self.lower_global_decl(symbol_id, global);
        }
    }

    fn lower_global_decl(&mut self, symbol_id: SymbolId, global_obj: GlobalObject) {
        let symbol = self.unit.symbols.get(symbol_id).unwrap().clone();

        let global = Operand::Global(Global {
            name: symbol.name.clone(),
            typ: symbol.qtype.typ.clone(),
        });

        self.globals
            .entry(symbol_id)
            .or_insert_with(|| global.clone());

        let initializer = if let Some(initializer) = global_obj.initializer {
            match initializer {
                tir::Initializer::Expr(expr) => Some(self.lower_global_constant(expr)),
            }
        } else {
            None
        };

        self.write_lir(Lir::Global {
            name: symbol.name.clone(),
            typ: symbol.qtype.typ.clone(),
            initializer,
        });
    }

    fn lower_global_constant(&self, expr: tir::Expr) -> ImmediateValue {
        match &expr.kind {
            tir::ExprKind::Literal(tir::LiteralKind::Int(value)) => {
                ImmediateValue::Int(*value as i32)
            }
            tir::ExprKind::Literal(tir::LiteralKind::Float(value)) => {
                ImmediateValue::Float(*value as f32)
            }
            tir::ExprKind::Literal(tir::LiteralKind::Double(value)) => {
                ImmediateValue::Double(*value as f64)
            }
            tir::ExprKind::Literal(tir::LiteralKind::Char(value)) => {
                ImmediateValue::Char(*value as u8)
            }

            _ => unreachable!("semantic analysis should guarantee a constant global initializer"),
        }
    }

    fn lower_function_def(&mut self, func: &tir::FunctionDef) {
        let symbol = self.unit.symbols.get(func.symbol_id).unwrap().clone();
        let Type::Function(function_type) = &symbol.qtype.typ else {
            unreachable!("function definition symbol must have a function type")
        };
        let return_type = function_type.return_type.typ.clone();

        self.stack_size = 0;
        self.locals.clear();
        self.live_starts.clear();
        self.live_intervals.clear();

        let start_index = self.instructions.len();

        let return_label = self.new_label();

        self.write_lir(Lir::FunctionStart {
            name: symbol.name.clone(),
            stack_size: 0,
        });

        // TODO: lower params

        self.lower_compound_stmt(func.body.clone(), &return_type, return_label.clone());

        assert!(
            self.live_starts.is_empty(),
            "all virtual registers must be freed before allocating a function"
        );

        let intervals = std::mem::take(&mut self.live_intervals);
        let allocation = RegisterAllocator::new(self.stack_size).allocate(&intervals);
        self.allocations.extend(allocation.allocations);

        if let Lir::FunctionStart { stack_size, .. } = &mut self.instructions[start_index] {
            *stack_size = allocation.stack_size;
        }

        self.write_lir(Lir::Label(return_label));
        self.write_lir(Lir::FunctionEnd);
    }

    fn lower_stmt(&mut self, stmt: tir::Stmt, return_type: &Type, return_label: LabelId) {
        match stmt {
            tir::Stmt::Compound(compound) => {
                self.lower_compound_stmt(compound, return_type, return_label);
            }
            tir::Stmt::Return { expr, .. } => {
                self.lower_return(expr.as_ref(), return_type, return_label);
            }
            tir::Stmt::Expression { expr, .. } => {
                let value = self.lower_expr(&expr);
                self.free(&value);
            }
        }
    }

    fn lower_compound_stmt(
        &mut self,
        compound: tir::CompoundStmt,
        return_type: &Type,
        return_label: LabelId,
    ) {
        for item in compound.items {
            match item {
                tir::BlockItem::Declaration(decl) => {
                    self.lower_local_declaration(&decl);
                }
                tir::BlockItem::Statement(stmt) => {
                    self.lower_stmt(stmt, return_type, return_label.clone());
                }
            }
        }
    }

    fn lower_local_declaration(&mut self, declaration: &tir::Declaration) {
        for declarator in &declaration.declarators {
            let typ = declarator.qtype.typ.clone();

            let slot = self.stack_slot(typ.clone());

            self.locals.insert(declarator.symbol_id, slot.clone());

            if let Some(initializer) = &declarator.initializer {
                let value = self.lower_initializer(initializer);

                let value = match value {
                    Operand::Stack(_) | Operand::Global(_) => self.make_temp(value),

                    value => value,
                };

                self.write_lir(Lir::Mov {
                    dst: slot,
                    src: value.clone(),
                });

                self.free(&value);
            }
        }
    }

    fn lower_return(
        &mut self,
        expr: Option<&tir::Expr>,
        return_type: &Type,
        return_label: LabelId,
    ) {
        if let Some(expr) = expr {
            let value = self.lower_expr(expr);

            let return_reg = Operand::Physical {
                reg: PhysicalReg::Rax,
                typ: return_type.clone(),
            };

            self.write_lir(Lir::Mov {
                src: value.clone(),
                dst: return_reg,
            });

            self.free(&value);
        }

        self.write_lir(Lir::Jmp(return_label));
    }

    fn lower_initializer(&mut self, initializer: &tir::Initializer) -> Operand {
        match initializer {
            tir::Initializer::Expr(expr) => self.lower_expr(expr),
        }
    }

    pub fn lower_expr(&mut self, expr: &tir::Expr) -> Operand {
        match &expr.kind {
            tir::ExprKind::Literal(lit) => match lit {
                tir::LiteralKind::Int(int_lit) => Operand::Immediate {
                    value: ImmediateValue::Int(*int_lit),
                    typ: expr.qtype.typ.clone(),
                },
                tir::LiteralKind::Float(float_lit) => Operand::Immediate {
                    value: ImmediateValue::Float(*float_lit),
                    typ: expr.qtype.typ.clone(),
                },
                tir::LiteralKind::Double(double_lit) => Operand::Immediate {
                    value: ImmediateValue::Double(*double_lit),
                    typ: expr.qtype.typ.clone(),
                },
                tir::LiteralKind::Char(char_lit) => Operand::Immediate {
                    value: ImmediateValue::Char(*char_lit),
                    typ: expr.qtype.typ.clone(),
                },
            },

            tir::ExprKind::Symbol(symbol_id) => self.get_var(symbol_id),

            tir::ExprKind::Binary { lhs, op, rhs } => {
                let left = self.lower_expr(lhs);
                let right = self.lower_expr(rhs);

                match op {
                    tir::BinaryOp::Add => self.lower_add(left, right, expr.qtype.typ.clone()),
                    tir::BinaryOp::Sub => self.lower_sub(left, right, expr.qtype.typ.clone()),
                    tir::BinaryOp::Mul => self.lower_mul(left, right, expr.qtype.typ.clone()),
                    tir::BinaryOp::Div => self.lower_div(left, right, expr.qtype.typ.clone(), true),

                    _ => todo!("binary operator {op:?}"),
                }
            }

            tir::ExprKind::Assignment { lhs, rhs } => {
                let destination = self.lower_lvalue(lhs);
                let value = self.lower_expr(rhs);

                let value = match value {
                    Operand::Stack(_) | Operand::Global(_) => self.make_temp(value),
                    other => other,
                };

                self.write_lir(Lir::Mov {
                    dst: destination.clone(),
                    src: value.clone(),
                });

                self.free(&value);

                destination
            }

            tir::ExprKind::Cast {
                new_type,
                expr: inner,
            } => self.lower_cast(inner, new_type.typ.clone()),

            _ => todo!("expression lowering for {:?}", expr.kind),
        }
    }

    fn lower_lvalue(&mut self, expr: &tir::Expr) -> Operand {
        match &expr.kind {
            tir::ExprKind::Symbol(symbol_id) => self.get_var(symbol_id),

            _ => todo!("lvalue lowering for {:?}", expr.kind),
        }
    }

    fn lower_add(&mut self, left: Operand, right: Operand, typ: Type) -> Operand {
        let result = self.temp(typ);

        self.write_lir(Lir::Mov {
            dst: result.clone(),
            src: left.clone(),
        });

        self.write_lir(Lir::Add {
            dst: result.clone(),
            src: right.clone(),
        });

        self.free(&left);
        self.free(&right);

        result
    }

    fn lower_sub(&mut self, left: Operand, right: Operand, typ: Type) -> Operand {
        let result = self.temp(typ);

        self.write_lir(Lir::Mov {
            dst: result.clone(),
            src: left.clone(),
        });

        self.write_lir(Lir::Sub {
            dst: result.clone(),
            src: right.clone(),
        });

        self.free(&left);
        self.free(&right);

        result
    }

    fn lower_mul(&mut self, left: Operand, right: Operand, typ: Type) -> Operand {
        let result = self.temp(typ);

        self.write_lir(Lir::Mov {
            dst: result.clone(),
            src: left.clone(),
        });

        self.write_lir(Lir::Mul {
            dst: result.clone(),
            src: right.clone(),
        });

        self.free(&left);
        self.free(&right);

        result
    }

    fn lower_div(&mut self, left: Operand, right: Operand, typ: Type, signed: bool) -> Operand {
        let divisor = self.make_temp(right);

        let rax = Operand::Physical {
            reg: PhysicalReg::Rax,
            typ: typ.clone(),
        };

        self.write_lir(Lir::Mov {
            dst: rax.clone(),
            src: left.clone(),
        });

        self.write_lir(Lir::Div {
            divisor: divisor.clone(),
            typ: typ.clone(),
            signed,
        });

        let result = self.temp(typ.clone());

        self.write_lir(Lir::Mov {
            dst: result.clone(),
            src: rax,
        });

        self.free(&left);
        self.free(&divisor);

        result
    }

    fn lower_cast(&mut self, expr: &tir::Expr, new_type: Type) -> Operand {
        let src = self.lower_expr(expr);

        let Some(old_type) = src.typ().cloned() else {
            return Operand::Void;
        };

        if old_type == new_type {
            return src;
        }

        let kind = cast_kind(&old_type, &new_type);
        let dst = self.temp(new_type);

        self.write_lir(Lir::Cast {
            kind,
            dst: dst.clone(),
            src: src.clone(),
        });

        self.free(&src);

        dst
    }
}

fn cast_kind(old: &Type, new: &Type) -> CastKind {
    match (old, new) {
        (Type::Primitive(Primitive::Int), Type::Primitive(Primitive::Float)) => {
            CastKind::IntToFloat
        }
        (Type::Primitive(Primitive::Int), Type::Primitive(Primitive::Double)) => {
            CastKind::IntToDouble
        }
        (Type::Primitive(Primitive::Float), Type::Primitive(Primitive::Int)) => {
            CastKind::FloatToInt
        }
        (Type::Primitive(Primitive::Double), Type::Primitive(Primitive::Int)) => {
            CastKind::DoubleToInt
        }
        (Type::Primitive(Primitive::Float), Type::Primitive(Primitive::Double)) => {
            CastKind::FloatToDouble
        }
        (Type::Primitive(Primitive::Double), Type::Primitive(Primitive::Float)) => {
            CastKind::DoubleToFloat
        }

        _ if old.size() < new.size() => CastKind::SignExtend,
        _ if old.size() > new.size() => CastKind::Truncate,
        _ => CastKind::ZeroExtend,
    }
}
