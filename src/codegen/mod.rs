use std::{collections::HashMap, fmt::Write};

use crate::semantic::{
    env::SymbolId,
    tir::{self, BinaryOp, BlockItem, ExprKind, ExternalDeclaration, Initializer, Stmt, UnaryOp},
};

#[derive(Default)]
pub struct Codegen {
    output: String,
    locals: HashMap<SymbolId, usize>,
    globals: HashMap<SymbolId, String>,
    return_label: String,
}

impl Codegen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn generate(mut self, unit: &tir::TranslationUnit) -> String {
        self.emit(".intel_syntax noprefix");
        self.emit_globals(unit);
        self.emit(".text");

        for declaration in &unit.external_declarations {
            if let ExternalDeclaration::FunctionDef(function) = declaration {
                self.emit_function(function);
            }
        }

        self.output
    }

    fn emit(&mut self, instruction: &str) {
        writeln!(self.output, "{instruction}").unwrap();
    }

    fn emit_globals(&mut self, unit: &tir::TranslationUnit) {
        let mut objects: Vec<(&str, Option<i32>)> = Vec::new();

        // indices to handle tentative definitions of the same global variable
        let mut indices = HashMap::new();

        for declaration in &unit.external_declarations {
            let ExternalDeclaration::Declaration(declaration) = declaration else {
                continue;
            };

            for global in &declaration.declarators {
                if global.qtype.typ.is_function() {
                    continue;
                }

                self.globals
                    .insert(global.symbol.id, global.symbol.name.clone());

                let value = global.initializer.as_ref().map(|initializer| {
                    let Initializer::Expr(expr) = initializer;

                    let ExprKind::IntLit(value) = &expr.kind else {
                        unreachable!("semantic analysis folds global initializers");
                    };

                    *value
                });

                let index = *indices
                    .entry(global.symbol.name.clone())
                    .or_insert_with(|| {
                        objects.push((&global.symbol.name, None));
                        objects.len() - 1
                    });

                // update the value if it is a definition (not tentative)
                if value.is_some() {
                    objects[index].1 = value;
                }
            }
        }

        for (name, value) in objects {
            let value = value.unwrap_or(0);
            if value == 0 {
                self.emit(&format!(".globl _{name}"));
                self.emit(&format!(".zerofill __DATA,__bss,_{name},4,2"));
            } else {
                self.emit(".data");
                self.emit(".p2align 2");
                self.emit(&format!(".globl _{name}"));
                self.emit(&format!("_{name}:"));
                self.emit(&format!("    .long {value}"));
            }
        }
    }

    fn allocate_locals(&mut self, compound: &tir::CompoundStmt) {
        for item in &compound.items {
            match item {
                BlockItem::Declaration(declaration) => {
                    for local in &declaration.declarators {
                        let offset = (self.locals.len() + 1) * 4;
                        self.locals.insert(local.symbol.id, offset);
                    }
                }
                BlockItem::Statement(Stmt::Compound(block)) => self.allocate_locals(block),
                BlockItem::Statement(_) => {}
            }
        }
    }

    fn emit_function(&mut self, function: &tir::FunctionDef) {
        self.locals.clear();
        self.allocate_locals(&function.body);

        let frame_size = (self.locals.len() * 4).div_ceil(16) * 16;
        self.return_label = format!("Lreturn_{}", function.symbol.id.0);
        let name = &function.symbol.name;

        self.emit(".p2align 4");
        self.emit(&format!(".globl _{name}"));
        self.emit(&format!("_{name}:"));
        self.emit("    push rbp");
        self.emit("    mov rbp, rsp");

        if frame_size != 0 {
            self.emit(&format!("    sub rsp, {frame_size}"));
        }

        self.emit_block(&function.body);

        if name == "main" {
            self.emit("    mov eax, 0");
        }

        self.emit(&format!("{}:", self.return_label));
        self.emit("    mov rsp, rbp");
        self.emit("    pop rbp");
        self.emit("    ret");
    }

    fn emit_block(&mut self, block: &tir::CompoundStmt) {
        for item in &block.items {
            match item {
                BlockItem::Declaration(declaration) => {
                    for local in &declaration.declarators {
                        if let Some(Initializer::Expr(expr)) = &local.initializer {
                            self.emit_expr(expr);
                            let offset = self.local_offset(local.symbol.id);
                            self.emit(&format!("    mov DWORD PTR [rbp - {offset}], eax"));
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
                self.emit(&format!("    jmp {}", self.return_label));
            }
        }
    }

    fn local_offset(&self, id: SymbolId) -> usize {
        self.locals.get(&id).copied().unwrap()
    }

    fn symbol_address(&self, id: SymbolId) -> String {
        if let Some(offset) = self.locals.get(&id) {
            format!("DWORD PTR [rbp - {offset}]")
        } else if let Some(name) = self.globals.get(&id) {
            format!("DWORD PTR [rip + _{name}]")
        } else {
            unreachable!("symbol must refer to an integer object");
        }
    }

    fn emit_expr(&mut self, expr: &tir::Expr) {
        match &expr.kind {
            ExprKind::IntLit(value) => {
                let value = value.clone() as i32;
                self.emit(&format!("    mov eax, {value}"));
            }
            ExprKind::Symbol(symbol) => {
                let address = self.symbol_address(symbol.id);
                self.emit(&format!("    mov eax, {address}"));
            }
            ExprKind::Unary { op, expr } => {
                self.emit_expr(expr);
                if matches!(op, UnaryOp::Neg) {
                    self.emit("    neg eax");
                }
            }
            ExprKind::Binary { lhs, op, rhs } => {
                self.emit_expr(lhs);
                self.emit("    push rax");
                self.emit_expr(rhs);
                self.emit("    mov ecx, eax");
                self.emit("    pop rax");

                // eax holds lhs; ecx holds rhs. Use 32-bit arithmetic for int.
                match op {
                    BinaryOp::Add => self.emit("    add eax, ecx"),
                    BinaryOp::Sub => self.emit("    sub eax, ecx"),
                    BinaryOp::Mul => self.emit("    imul eax, ecx"),
                    BinaryOp::Div => {
                        self.emit("    cdq");
                        self.emit("    idiv ecx");
                    }
                }
            }
            ExprKind::Assignment { lhs, rhs } => {
                let symbol = match &lhs.kind {
                    tir::ExprKind::Symbol(symbol) => symbol,
                    _ => unreachable!("assignment lhs must be a symbol"),
                };

                let address = self.symbol_address(symbol.id);
                self.emit_expr(rhs);
                self.emit(&format!("    mov {address}, eax"));
            }
        }
    }
}
