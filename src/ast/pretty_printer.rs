use crate::ast::*;

pub struct PrettyPrinter {
    output: String,
}

const BLUE: &str = "\x1b[34m";
const GREY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

impl PrettyPrinter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
        }
    }

    pub fn print(mut self, program: &TranslationUnit) -> String {
        self.output.push_str(&format!("AST Output\n"));
        self.line("", true, &format!("{BLUE}Translation Unit{RESET}"));

        let len = program.external_declarations.len();

        let prefix = Self::child_prefix("", true);

        for (i, decl) in program.external_declarations.iter().enumerate() {
            self.print_external_decl(decl, &prefix, i == len - 1);
        }

        self.output
    }

    fn line(&mut self, prefix: &str, last: bool, text: &str) {
        self.output.push_str(prefix);

        if last {
            self.output.push_str("└── ");
        } else {
            self.output.push_str("├── ");
        }

        self.output.push_str(text);
        self.output.push('\n');
    }

    fn child_prefix(prefix: &str, last: bool) -> String {
        if last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        }
    }

    fn print_span(&mut self, span: &Span, prefix: &str, last: bool) {
        self.line(
            prefix,
            last,
            &format!("{GREY}start: {}, end: {}{RESET}", span.start, span.end),
        );
    }

    fn print_external_decl(&mut self, decl: &ExternalDeclaration, prefix: &str, last: bool) {
        match decl {
            ExternalDeclaration::FunctionDef(function) => {
                self.print_function_def(function, prefix, last);
            }
            ExternalDeclaration::Declaration(declaration) => {
                self.print_decl(declaration, prefix, last);
            }
        }
    }

    fn print_function_def(&mut self, function: &FunctionDef, prefix: &str, last: bool) {
        self.line(prefix, last, &format!("{BLUE}FunctionDef{RESET}"));

        let prefix = Self::child_prefix(prefix, last);

        self.print_span(&function.span, &prefix, false);
        self.print_decl_specifiers(&function.specifiers, &prefix, false);
        self.print_declarator(&function.declarator, &prefix, false);
        self.print_compound_stmt(&function.body, &prefix, true);
    }

    fn print_decl(&mut self, declaration: &Declaration, prefix: &str, last: bool) {
        self.line(prefix, last, &format!("{BLUE}Declaration{RESET}"));

        let prefix = Self::child_prefix(prefix, last);

        self.print_span(&declaration.span, &prefix, false);
        self.print_decl_specifiers(&declaration.specifiers, &prefix, false);

        let len = declaration.declarators.len();

        for (i, declarator) in declaration.declarators.iter().enumerate() {
            self.print_init_declarator(declarator, &prefix, i == len - 1);
        }
    }

    fn print_init_declarator(&mut self, init_decl: &InitDeclarator, prefix: &str, last: bool) {
        self.line(prefix, last, &format!("{BLUE}InitDeclarator{RESET}"));

        let prefix = Self::child_prefix(prefix, last);

        self.print_span(&init_decl.span, &prefix, false);
        self.print_declarator(
            &init_decl.declarator,
            &prefix,
            init_decl.initializer.is_none(),
        );

        if let Some(initializer) = &init_decl.initializer {
            self.line(&prefix, true, "Initializer");

            let init_prefix = Self::child_prefix(&prefix, true);
            self.print_span(initializer.span(), &init_prefix, false);

            match initializer {
                Initializer::Expr(_, expr) => {
                    self.print_expr(expr, &init_prefix, true);
                }
            }
        }
    }

    fn print_decl_specifiers(&mut self, specifiers: &DeclSpecifiers, prefix: &str, last: bool) {
        self.line(prefix, last, &format!("{BLUE}DeclSpecifiers{RESET}"));

        let prefix = Self::child_prefix(prefix, last);
        let len = specifiers.type_specifiers.len();

        self.print_span(&specifiers.span, &prefix, false);

        for (i, specifier) in specifiers.type_specifiers.iter().enumerate() {
            let name = match specifier {
                TypeSpecifier::Void => "Void",
                TypeSpecifier::Int => "Int",
                TypeSpecifier::Float => "Float",
                TypeSpecifier::Double => "Double",
                TypeSpecifier::Char => "Char",
            };

            self.line(&prefix, i == len - 1, name);
        }
    }

    fn print_declarator(&mut self, declarator: &Declarator, prefix: &str, last: bool) {
        match declarator {
            Declarator::Identifier(span, name) => {
                self.print_identifier(&span, &name, prefix, last);
            }

            Declarator::Function(span, inner, params) => {
                self.line(prefix, last, &format!("{BLUE}Function{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                self.print_span(&span, &prefix, false);
                self.print_declarator(inner, &prefix, params.is_empty());

                if !params.is_empty() {
                    self.line(&prefix, true, "Parameters");

                    let param_prefix = Self::child_prefix(&prefix, true);
                    let len = params.len();

                    for (i, param) in params.iter().enumerate() {
                        self.print_declarator(param, &param_prefix, i == len - 1);
                    }
                }
            }
        }
    }

    fn print_compound_stmt(&mut self, stmt: &CompoundStmt, prefix: &str, last: bool) {
        self.line(prefix, last, &format!("{BLUE}CompoundStmt{RESET}"));

        let prefix = Self::child_prefix(prefix, last);
        let len = stmt.items.len();

        self.print_span(&stmt.span, &prefix, false);
        for (i, item) in stmt.items.iter().enumerate() {
            match item {
                BlockItem::Declaration(decl) => {
                    self.print_decl(decl, &prefix, i == len - 1);
                }
                BlockItem::Statement(stmt) => {
                    self.print_stmt(stmt, &prefix, i == len - 1);
                }
            }
        }
    }

    fn print_stmt(&mut self, stmt: &Stmt, prefix: &str, last: bool) {
        match stmt {
            Stmt::Expression(span, expr) => {
                self.print_expr(expr, prefix, last);
            }
            Stmt::Compound(compound) => {
                self.print_compound_stmt(compound, prefix, last);
            }

            Stmt::Return(span, expr) => {
                self.line(prefix, last, &format!("{BLUE}Return{RESET}"));

                if let Some(expr) = expr {
                    let prefix = Self::child_prefix(prefix, last);

                    self.print_span(&span, &prefix, false);
                    self.print_expr(expr, &prefix, true);
                }
            }
        }
    }

    fn print_identifier(&mut self, span: &Span, name: &str, prefix: &str, last: bool) {
        self.line(prefix, last, &format!("Identifier"));

        let prefix = Self::child_prefix(prefix, last);

        self.print_span(&span, &prefix, false);
        self.line(&prefix, last, &format!("Name: ({})", name));
    }

    fn print_expr(&mut self, expr: &Expr, prefix: &str, last: bool) {
        match expr {
            Expr::IntLit(span, value) => {
                self.line(prefix, last, &format!("{BLUE}IntLit{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                self.print_span(&span, &prefix, false);
                self.line(&prefix, last, &format!("Int: ({}){RESET}", value));
            }
            Expr::FloatLit(span, value, suffix) => {
                self.line(prefix, last, &format!("{BLUE}FloatLit{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                let suffix = match suffix {
                    FloatSuffix::None => "",
                    FloatSuffix::Float => "f",
                };

                self.print_span(&span, &prefix, false);
                self.line(
                    &prefix,
                    last,
                    &format!("Float: ({}{}){RESET}", value, suffix),
                );
            }
            Expr::CharLit(span, value) => {
                self.line(prefix, last, &format!("{BLUE}CharLit{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                self.print_span(&span, &prefix, false);
                self.line(&prefix, last, &format!("Char: ({}){RESET}", value));
            }
            Expr::Identifier(span, name) => {
                self.print_identifier(&span, &name, prefix, last);
            }
            Expr::Unary(span, op, expr) => {
                self.line(prefix, last, &format!("{BLUE}Unary{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                self.line(&prefix, false, &format!("Operator: ({op:?})"));
                self.print_span(&span, &prefix, false);
                self.print_expr(expr, &prefix, true);
            }
            Expr::Binary(span, lhs, op, rhs) => {
                self.line(prefix, last, &format!("{BLUE}Binary{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                self.print_span(&span, &prefix, false);
                self.line(&prefix, false, &format!("Operator: ({op:?})"));
                self.print_expr(lhs, &prefix, false);
                self.print_expr(rhs, &prefix, true);
            }
            Expr::Assignment(span, lhs, rhs) => {
                self.line(prefix, last, &format!("{BLUE}Assignment{RESET}"));

                let prefix = Self::child_prefix(prefix, last);

                self.print_span(&span, &prefix, false);
                self.print_expr(lhs, &prefix, false);
                self.print_expr(rhs, &prefix, true);
            }
        }
    }
}
