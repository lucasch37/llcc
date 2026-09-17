use crate::semantic::{
    env::SymbolId,
    tir::{
        self, BinaryOp, BlockItem, ExprKind, ExternalDeclaration, Initializer, Stmt, UnaryOp, Value,
    },
    types::{Primitive, Type},
};
use std::{collections::HashMap, fmt::Write};

#[derive(Default)]
pub struct Codegen {
    output: String,
    locals: HashMap<SymbolId, usize>,
    globals: HashMap<SymbolId, String>,
    return_label: String,
    frame_used: usize,
}

impl Codegen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn generate(mut self, unit: &tir::TranslationUnit) -> String {
        self.line(".intel_syntax noprefix");

        self.emit_globals_vars(unit);

        self.line(".text");

        for declaration in &unit.external_declarations {
            if let ExternalDeclaration::FunctionDef(function) = declaration {
                self.emit_function(function);
            }
        }

        self.output
    }

    fn line(&mut self, value: &str) {
        writeln!(self.output, "{value}").unwrap();
    }

    fn get_primitive(typ: &Type) -> &Primitive {
        let Type::Primitive(primitive) = typ else {
            unreachable!("expected scalar type")
        };

        primitive
    }

    fn constant(initializer: &Initializer) -> Value {
        let Initializer::Expr(expr) = initializer;
        match expr.kind {
            ExprKind::Value(value) => value,
            _ => unreachable!("global initializer was not folded"),
        }
    }

    fn emit_globals_vars(&mut self, unit: &tir::TranslationUnit) {
        let mut objects: Vec<(&str, Value)> = vec![];
        // indices to handle tentative definitions of the same global variable
        let mut indices = HashMap::new();

        for external in &unit.external_declarations {
            let ExternalDeclaration::Declaration(declaration) = external else {
                continue;
            };

            for global in &declaration.declarators {
                if global.qtype.typ.is_function() {
                    continue;
                }

                self.globals
                    .insert(global.symbol.id, global.symbol.name.clone());

                let zero = match Self::get_primitive(&global.qtype.typ) {
                    Primitive::Int => Value::Int(0),
                    Primitive::Float => Value::Float(0.0),
                    Primitive::Double => Value::Double(0.0),
                    Primitive::Char => Value::Char(0),
                    Primitive::Void => unreachable!(),
                };

                let index = *indices
                    .entry(global.symbol.name.clone())
                    .or_insert_with(|| {
                        objects.push((&global.symbol.name, zero));
                        objects.len() - 1
                    });

                // update the value if it is a definition (not tentative)
                if let Some(initializer) = &global.initializer {
                    objects[index].1 = Self::constant(initializer);
                }
            }
        }

        for (name, value) in objects {
            let (size, zero) = match value {
                Value::Char(v) => (1, v == 0),
                Value::Int(v) => (4, v == 0),
                Value::Float(v) => (4, v == 0.0),
                Value::Double(v) => (8, v == 0.0),
            };

            if zero {
                self.line(&format!(".globl _{name}"));
                self.line(&format!(
                    ".zerofill __DATA,__bss,_{name},{size},{}",
                    if size == 8 {
                        3
                    } else if size == 4 {
                        2
                    } else {
                        0
                    }
                ));
                continue;
            }

            self.line(".data");
            self.line(if size == 8 {
                ".p2align 3"
            } else if size == 4 {
                ".p2align 2"
            } else {
                ".p2align 0"
            });
            self.line(&format!(".globl _{name}"));
            self.line(&format!("_{name}:"));

            match value {
                Value::Char(v) => self.line(&format!("    .byte {v}")),
                Value::Int(v) => self.line(&format!("    .long {v}")),
                Value::Float(v) => self.line(&format!("    .long {}", v.to_bits())),
                Value::Double(v) => self.line(&format!("    .quad {}", v.to_bits())),
            }
        }
    }

    fn allocate_locals(&mut self, block: &tir::CompoundStmt) {
        for item in &block.items {
            match item {
                BlockItem::Declaration(declaration) => {
                    for local in &declaration.declarators {
                        let size = local.qtype.typ.size();
                        self.frame_used = self.frame_used.div_ceil(size) * size + size;
                        self.locals.insert(local.symbol.id, self.frame_used);
                    }
                }
                BlockItem::Statement(Stmt::Compound(block)) => self.allocate_locals(block),
                BlockItem::Statement(_) => {}
            }
        }
    }

    fn emit_function(&mut self, function: &tir::FunctionDef) {
        self.locals.clear();
        self.frame_used = 0;

        self.allocate_locals(&function.body);

        let frame_size = self.frame_used.div_ceil(16) * 16;
        let name = &function.symbol.name;

        self.return_label = format!("Lreturn_{}", function.symbol.id.0);

        self.line(".p2align 4");
        self.line(&format!(".globl _{name}"));
        self.line(&format!("_{name}:"));
        self.line("    push rbp");
        self.line("    mov rbp, rsp");

        if frame_size > 0 {
            self.line(&format!("    sub rsp, {frame_size}"));
        }

        self.emit_block(&function.body);

        if name == "main" {
            self.line("    mov eax, 0");
        }

        self.line(&format!("{}:", self.return_label));
        self.line("    mov rsp, rbp");
        self.line("    pop rbp");
        self.line("    ret");
    }

    fn emit_block(&mut self, block: &tir::CompoundStmt) {
        for item in &block.items {
            match item {
                BlockItem::Declaration(declaration) => {
                    for local in &declaration.declarators {
                        if let Some(Initializer::Expr(expr)) = &local.initializer {
                            self.emit_expr(expr);
                            self.store(local.symbol.id, &local.qtype.typ);
                        }
                    }
                }
                BlockItem::Statement(stmt) => self.emit_stmt(stmt),
            }
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Compound(block) => self.emit_block(block),
            Stmt::Expression { expr, .. } => self.emit_expr(expr),
            Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    self.emit_expr(expr);
                }
                self.line(&format!("    jmp {}", self.return_label));
            }
        }
    }

    fn address(&self, id: SymbolId) -> String {
        if let Some(offset) = self.locals.get(&id) {
            format!("[rbp - {offset}]")
        } else if let Some(name) = self.globals.get(&id) {
            format!("[rip + _{name}]")
        } else {
            unreachable!("unknown object symbol")
        }
    }

    fn load(&mut self, id: SymbolId, typ: &Type) {
        let address = self.address(id);
        self.line(&match Self::get_primitive(typ) {
            Primitive::Int => format!("    mov eax, DWORD PTR {address}"),
            Primitive::Float => format!("    movss xmm0, DWORD PTR {address}"),
            Primitive::Double => format!("    movsd xmm0, QWORD PTR {address}"),
            Primitive::Char => format!("    movsx eax, BYTE PTR {address}"),
            Primitive::Void => unreachable!(),
        });
    }

    fn store(&mut self, id: SymbolId, typ: &Type) {
        let address = self.address(id);
        self.line(&match Self::get_primitive(typ) {
            Primitive::Int => format!("    mov DWORD PTR {address}, eax"),
            Primitive::Float => format!("    movss DWORD PTR {address}, xmm0"),
            Primitive::Double => format!("    movsd QWORD PTR {address}, xmm0"),
            Primitive::Char => format!("    mov BYTE PTR {address}, al"),
            Primitive::Void => unreachable!(),
        });
    }

    fn emit_global_var(&mut self, value: Value) {
        match value {
            Value::Char(value) => self.line(&format!("    mov eax, {value}")),
            Value::Int(value) => self.line(&format!("    mov eax, {value}")),
            Value::Float(value) => {
                self.line(&format!("    mov eax, {}", value.to_bits()));
                self.line("    movd xmm0, eax");
            }
            Value::Double(value) => {
                self.line(&format!("    movabs rax, {}", value.to_bits()));
                self.line("    movq xmm0, rax");
            }
        }
    }

    fn emit_expr(&mut self, expr: &tir::Expr) {
        match &expr.kind {
            ExprKind::Value(value) => self.emit_global_var(*value),
            ExprKind::Symbol(symbol) => self.load(symbol.id, &symbol.qtype.typ),
            ExprKind::Cast {
                new_type,
                expr: inner,
            } => {
                self.emit_expr(inner);
                match (
                    Self::get_primitive(&inner.qtype.typ),
                    Self::get_primitive(&new_type.typ),
                ) {
                    // castings
                    (Primitive::Int, Primitive::Float) => self.line("    cvtsi2ss xmm0, eax"),
                    (Primitive::Int, Primitive::Double) => self.line("    cvtsi2sd xmm0, eax"),
                    (Primitive::Float, Primitive::Int) => self.line("    cvttss2si eax, xmm0"),
                    (Primitive::Double, Primitive::Int) => self.line("    cvttsd2si eax, xmm0"),
                    (Primitive::Float, Primitive::Double) => self.line("    cvtss2sd xmm0, xmm0"),
                    (Primitive::Double, Primitive::Float) => self.line("    cvtsd2ss xmm0, xmm0"),
                    (Primitive::Char, Primitive::Int) => {}
                    (Primitive::Int, Primitive::Char) => self.line("    movsx eax, al"),
                    (Primitive::Char, Primitive::Float) => self.line("    cvtsi2ss xmm0, eax"),
                    (Primitive::Char, Primitive::Double) => self.line("    cvtsi2sd xmm0, eax"),
                    (Primitive::Float, Primitive::Char) => {
                        self.line("    cvttss2si eax, xmm0");
                        self.line("    movsx eax, al");
                    }
                    (Primitive::Double, Primitive::Char) => {
                        self.line("    cvttsd2si eax, xmm0");
                        self.line("    movsx eax, al");
                    }
                    (source, target) if source == target => {}
                    _ => unreachable!("invalid cast"),
                }
            }
            ExprKind::Unary { op, expr: inner } => {
                self.emit_expr(inner);

                if matches!(op, UnaryOp::Neg) {
                    self.negative(Self::get_primitive(&expr.qtype.typ));
                }
            }
            ExprKind::Binary { lhs, op, rhs } => self.emit_binary(expr, lhs, *op, rhs),
            ExprKind::Assignment { lhs, rhs } => {
                let ExprKind::Symbol(symbol) = &lhs.kind else {
                    unreachable!("Assignment target must be a symbol")
                };

                self.emit_expr(rhs);
                self.store(symbol.id, &symbol.qtype.typ);
            }
        }
    }

    // make the value in eax/xmm0 negative, depending on the type
    fn negative(&mut self, typ: &Primitive) {
        match typ {
            Primitive::Int => self.line("    neg eax"),
            Primitive::Char => {
                self.line("    neg eax");
                self.line("    movsx eax, al");
            }
            Primitive::Float => {
                self.line("    mov eax, 2147483648");
                self.line("    movd xmm1, eax");
                self.line("    xorps xmm0, xmm1");
            }
            Primitive::Double => {
                self.line("    movabs rax, 9223372036854775808");
                self.line("    movq xmm1, rax");
                self.line("    xorpd xmm0, xmm1");
            }
            Primitive::Void => unreachable!(),
        }
    }

    fn emit_binary(&mut self, whole: &tir::Expr, lhs: &tir::Expr, op: BinaryOp, rhs: &tir::Expr) {
        self.emit_expr(lhs);

        if matches!(
            Self::get_primitive(&whole.qtype.typ),
            Primitive::Int | Primitive::Char
        ) {
            self.line("    push rax");
            self.emit_expr(rhs);
            self.line("    mov ecx, eax");
            self.line("    pop rax");

            match op {
                BinaryOp::Add => self.line("    add eax, ecx"),
                BinaryOp::Sub => self.line("    sub eax, ecx"),
                BinaryOp::Mul => self.line("    imul eax, ecx"),
                BinaryOp::Div => {
                    self.line("    cdq");
                    self.line("    idiv ecx");
                }
            }

            if matches!(Self::get_primitive(&whole.qtype.typ), Primitive::Char) {
                self.line("    movsx eax, al");
            }

            return;
        }

        // whether to use float or double logic
        let is_float = matches!(Self::get_primitive(&whole.qtype.typ), Primitive::Float);

        let mov = if is_float { "movss" } else { "movsd" };
        let ptr = if is_float { "DWORD" } else { "QWORD" };

        self.line("    sub rsp, 16");
        self.line(&format!("    {mov} {ptr} PTR [rsp], xmm0"));
        self.emit_expr(rhs);
        self.line("    movaps xmm1, xmm0");
        self.line(&format!("    {mov} xmm0, {ptr} PTR [rsp]"));
        self.line("    add rsp, 16");

        let instruction = match (is_float, op) {
            (true, BinaryOp::Add) => "addss",
            (true, BinaryOp::Sub) => "subss",
            (true, BinaryOp::Mul) => "mulss",
            (true, BinaryOp::Div) => "divss",
            (false, BinaryOp::Add) => "addsd",
            (false, BinaryOp::Sub) => "subsd",
            (false, BinaryOp::Mul) => "mulsd",
            (false, BinaryOp::Div) => "divsd",
        };

        self.line(&format!("    {instruction} xmm0, xmm1"));
    }
}
