use crate::{
    ast,
    error::Result,
    error::{Error, ErrorKind},
    semantic::types::{
        FunctionType,
        Primitive::{self, Void},
        QualType, Type,
    },
    token::Span,
};

pub(crate) mod env;
mod fold;
pub(crate) mod tir;
pub mod types;

pub struct SemanticAnalyzer<'a> {
    ast: &'a ast::TranslationUnit,
    source: &'a str,
    filename: &'a str,

    scope_tree: env::ScopeTree,
    errors: Vec<Error>,

    current_function: Option<FunctionContext>,
}

struct FunctionContext {
    return_type: QualType,
}

impl<'a> SemanticAnalyzer<'a> {
    pub fn new(ast: &'a ast::TranslationUnit, source: &'a str, filename: &'a str) -> Self {
        Self {
            ast,
            source,
            filename,
            scope_tree: env::ScopeTree::new(),
            errors: Vec::new(),
            current_function: None,
        }
    }

    fn error(&mut self, span: Span, kind: ErrorKind) -> Error {
        Error::new(
            self.source.to_string(),
            self.filename.to_string(),
            span,
            kind,
        )
    }

    pub fn analyze(mut self) -> Result<tir::TranslationUnit> {
        let tir = self.lower_translation_unit();

        if !self.errors.is_empty() {
            return Err(Error::new_multiple(self.errors));
        }

        Ok(tir)
    }

    pub fn lower_translation_unit(&mut self) -> tir::TranslationUnit {
        let mut external_declarations = Vec::new();

        for ext_decl in &self.ast.external_declarations {
            match ext_decl {
                ast::ExternalDeclaration::FunctionDef(func_def) => {
                    match self.lower_function_def(func_def) {
                        Ok(tir_func_def) => external_declarations
                            .push(tir::ExternalDeclaration::FunctionDef(tir_func_def)),
                        Err(err) => self.errors.push(err),
                    }
                }
                ast::ExternalDeclaration::Declaration(decl) => match self.lower_declaration(decl) {
                    Ok(tir_decl) => {
                        external_declarations.push(tir::ExternalDeclaration::Declaration(tir_decl))
                    }
                    Err(err) => self.errors.push(err),
                },
            }
        }

        tir::TranslationUnit {
            symbols: self.scope_tree.symbols().clone(),
            external_declarations,
        }
    }

    pub fn lower_declaration(&mut self, decl: &ast::Declaration) -> Result<tir::Declaration> {
        let qtype = self.resolve_decl_specifiers(&decl.specifiers);
        let mut declarators = Vec::new();

        for decl in &decl.declarators {
            match self.lower_init_declarator(decl.clone(), qtype.clone()) {
                Ok(tir_decl) => declarators.push(tir_decl),
                Err(err) => self.errors.push(err),
            }
        }

        Ok(tir::Declaration {
            span: decl.span,
            declarators,
        })
    }

    pub fn resolve_decl_specifiers(&self, specifiers: &ast::DeclSpecifiers) -> QualType {
        let typ = match specifiers.type_specifiers[0] {
            ast::TypeSpecifier::Void => Type::Primitive(Primitive::Void),
            ast::TypeSpecifier::Int => Type::Primitive(Primitive::Int),
            ast::TypeSpecifier::Float => Type::Primitive(Primitive::Float),
            ast::TypeSpecifier::Double => Type::Primitive(Primitive::Double),
            ast::TypeSpecifier::Char => Type::Primitive(Primitive::Char),
        };

        QualType::new(typ)
    }

    pub fn lower_init_declarator(
        &mut self,
        declarator: ast::InitDeclarator,
        base_qtype: QualType,
    ) -> Result<tir::InitDeclarator> {
        let ast::InitDeclarator {
            span,
            declarator,
            initializer,
        } = declarator;

        let (name, qtype) = self.resolve_declarator(declarator, base_qtype);

        if qtype.typ.is_void() {
            return Err(self.error(span, ErrorKind::IncompleteType(qtype)));
        }

        let kind = if initializer.is_some() {
            env::InitType::Definition
        } else {
            env::InitType::Declaration
        };

        let existing = self.scope_tree.lookup_local(&name);
        if let Some(existing) = existing {
            if self.scope_tree.check_redefinition(&qtype, &kind, existing) {
                return Err(self.error(span, ErrorKind::Redefinition(name)));
            }
        }

        let symbol_id = match existing {
            Some(existing_id) if self.scope_tree.is_global() => {
                if kind == env::InitType::Definition {
                    self.scope_tree.symbol_mut(existing_id).kind = env::InitType::Definition;
                }
                existing_id
            }
            _ => self
                .scope_tree
                .new_symbol(name.clone(), qtype.clone(), kind),
        };

        self.scope_tree.define(name, symbol_id);

        let mut initializer = initializer
            .map(|initializer| match initializer {
                ast::Initializer::Expr(_, expr) => {
                    let expr = self.lower_expr(expr.clone())?;
                    let expr = expr.cast_to(qtype.clone());
                    let expr = self.fold_expr(expr)?;

                    Ok(tir::Initializer::Expr(expr))
                }
            })
            .transpose()?;

        if let Some(tir::Initializer::Expr(expr)) = initializer.as_ref() {
            if qtype.typ.is_function() {
                return Err(self.error(span, ErrorKind::InitializedFunction));
            }
        }

        Ok(tir::InitDeclarator {
            span,
            symbol_id,
            qtype,
            initializer,
        })
    }

    fn resolve_declarator(
        &self,
        declarator: ast::Declarator,
        base_qtype: QualType,
    ) -> (String, QualType) {
        match declarator {
            ast::Declarator::Identifier(_, name) => (name, base_qtype),
            ast::Declarator::Function(_, inner, params) => {
                let params = params
                    .into_iter()
                    .map(|param| {
                        self.resolve_declarator(
                            param,
                            QualType::new(Type::Primitive(Primitive::Int)),
                        )
                        .1
                    })
                    .collect();

                let function_qtype = QualType::new(Type::Function(FunctionType {
                    return_type: Box::new(base_qtype),
                    params,
                    variadic: false,
                }));

                self.resolve_declarator(*inner, function_qtype)
            }
        }
    }

    fn decode_char_literal(literal: &str) -> u8 {
        let contents = literal
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
            .expect("validated character literal");

        let value = match contents.as_bytes() {
            [value] => *value,
            [b'\\', b'n'] => b'\n',
            [b'\\', b't'] => b'\t',
            [b'\\', b'r'] => b'\r',
            [b'\\', b'0'] => b'\0',
            [b'\\', b'\\'] => b'\\',
            [b'\\', b'\''] => b'\'',
            bytes => *bytes.last().expect("nonempty character literal"),
        };

        value as u8
    }

    pub fn lower_expr(&mut self, expr: ast::Expr) -> Result<tir::Expr> {
        let mut expr = match expr {
            ast::Expr::Binary(span, lhs, op, rhs) => self.lower_binary(span, *lhs, op, *rhs),
            ast::Expr::IntLit(span, lit) => Ok(tir::Expr {
                span,
                qtype: QualType::new(Type::Primitive(Primitive::Int)),
                kind: tir::ExprKind::Literal(tir::LiteralKind::Int(
                    lit.parse().expect("validated integer literal"),
                )),
                value_kind: tir::ValueKind::RValue,
            }),
            ast::Expr::CharLit(span, lit) => Ok(tir::Expr {
                span,
                qtype: QualType::new(Type::Primitive(Primitive::Char)),
                kind: tir::ExprKind::Literal(tir::LiteralKind::Char(Self::decode_char_literal(
                    &lit,
                ))),
                value_kind: tir::ValueKind::RValue,
            }),
            ast::Expr::FloatLit(span, lit, suffix) => match suffix {
                ast::FloatSuffix::None => Ok(tir::Expr {
                    span,
                    qtype: QualType::new(Type::Primitive(Primitive::Double)),
                    kind: tir::ExprKind::Literal(tir::LiteralKind::Double(
                        lit.parse().expect("validated float literal"),
                    )),
                    value_kind: tir::ValueKind::RValue,
                }),
                ast::FloatSuffix::Float => Ok(tir::Expr {
                    span,
                    qtype: QualType::new(Type::Primitive(Primitive::Float)),
                    kind: tir::ExprKind::Literal(tir::LiteralKind::Float(
                        lit.parse().expect("validated float literal"),
                    )),
                    value_kind: tir::ValueKind::RValue,
                }),
            },
            ast::Expr::Identifier(span, name) => {
                let Some(symbol) = self.scope_tree.lookup(&name) else {
                    return Err(self.error(span, ErrorKind::UndeclaredIdent(name.clone())));
                };
                let qtype = self.scope_tree.symbol(symbol).qtype.clone();

                Ok(tir::Expr {
                    span,
                    qtype,
                    kind: tir::ExprKind::Symbol(symbol),
                    value_kind: tir::ValueKind::LValue,
                })
            }
            ast::Expr::Unary(span, op, expr) => {
                let expr = Box::new(self.lower_expr(*expr)?);
                let qtype = expr.qtype.clone();

                let op = match op {
                    ast::UnaryOp::Pos => tir::UnaryOp::Pos,
                    ast::UnaryOp::Neg => tir::UnaryOp::Neg,
                };

                Ok(tir::Expr {
                    span,
                    qtype,
                    kind: tir::ExprKind::Unary { op, expr },
                    value_kind: tir::ValueKind::RValue,
                })
            }
            ast::Expr::Assignment(span, lhs, rhs) => {
                let lhs = self.lower_expr(*lhs)?;
                let rhs = self.lower_expr(*rhs)?;

                if !matches!(lhs.kind, tir::ExprKind::Symbol(_)) {
                    return Err(self.error(lhs.span, ErrorKind::InvalidAssignmentTarget));
                }

                if !lhs.qtype.type_compatible(&rhs.qtype) {
                    return Err(self.error(
                        span,
                        ErrorKind::TypeMismatch(lhs.qtype.clone(), rhs.qtype.clone()),
                    ));
                }

                let rhs = rhs.cast_to(lhs.qtype.clone());

                Ok(tir::Expr {
                    span,
                    qtype: lhs.qtype.clone(),
                    kind: tir::ExprKind::Assignment {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    value_kind: tir::ValueKind::RValue,
                })
            }

            _ => todo!(),
        };

        expr = self.fold_expr(expr?);

        expr
    }

    pub fn lower_binary(
        &mut self,
        span: Span,
        lhs: ast::Expr,
        op: ast::BinaryOp,
        rhs: ast::Expr,
    ) -> Result<tir::Expr> {
        let left = self.lower_expr(lhs)?;
        let right = self.lower_expr(rhs)?;

        let left = left.int_promote();
        let right = right.int_promote();

        let (left, right) = Self::arithmetic_promotion(left, right);

        Ok(tir::Expr {
            span,
            qtype: left.qtype.clone(),
            kind: tir::ExprKind::Binary {
                lhs: Box::new(left),
                op: match op {
                    ast::BinaryOp::Add => tir::BinaryOp::Add,
                    ast::BinaryOp::Sub => tir::BinaryOp::Sub,
                    ast::BinaryOp::Mul => tir::BinaryOp::Mul,
                    ast::BinaryOp::Div => tir::BinaryOp::Div,
                },
                rhs: Box::new(right),
            },
            value_kind: tir::ValueKind::RValue,
        })
    }

    pub fn arithmetic_promotion(left: tir::Expr, right: tir::Expr) -> (tir::Expr, tir::Expr) {
        if left.qtype.typ.is_void() || right.qtype.typ.is_void() {
            return (left, right);
        }

        match (&left.qtype.typ, &right.qtype.typ) {
            (Type::Primitive(Primitive::Double), _) | (_, Type::Primitive(Primitive::Double)) => {
                let left = left.cast_to(QualType::new(Type::Primitive(Primitive::Double)));
                let right = right.cast_to(QualType::new(Type::Primitive(Primitive::Double)));
                (left, right)
            }

            (Type::Primitive(Primitive::Float), _) | (_, Type::Primitive(Primitive::Float)) => {
                let left = left.cast_to(QualType::new(Type::Primitive(Primitive::Float)));
                let right = right.cast_to(QualType::new(Type::Primitive(Primitive::Float)));
                (left, right)
            }

            _ => (left, right),
        }
    }

    pub fn lower_function_def(
        &mut self,
        function_def: &ast::FunctionDef,
    ) -> Result<tir::FunctionDef> {
        let return_type = self.resolve_decl_specifiers(&function_def.specifiers);
        let name = self.declarator_name(&function_def.declarator);

        let func_type = FunctionType {
            return_type: Box::new(return_type),
            params: Vec::new(),
            variadic: false,
        };

        let qtype = QualType::new(Type::Function(func_type.clone()));
        let kind = env::InitType::Definition;
        let existing = self.scope_tree.lookup(&name);

        if let Some(existing) = existing {
            if self.scope_tree.check_redefinition(&qtype, &kind, existing) {
                return Err(self.error(function_def.span, ErrorKind::Redefinition(name)));
            }
        }

        let symbol = match existing {
            Some(existing) => {
                self.scope_tree.symbol_mut(existing).kind = env::InitType::Definition;
                existing
            }
            None => self
                .scope_tree
                .new_symbol(name.clone(), qtype.clone(), kind),
        };

        self.current_function = Some(FunctionContext {
            return_type: func_type.return_type.as_ref().clone(),
        });

        self.scope_tree.define(name, symbol);

        self.scope_tree.enter();

        let compound_stmt = self.lower_compound_stmt(&function_def.body, false)?;

        self.scope_tree.exit();

        Ok(tir::FunctionDef {
            span: function_def.span,
            symbol_id: symbol,
            qtype,
            params: Vec::new(),
            body: compound_stmt,
        })
    }

    fn declarator_name(&self, declarator: &ast::Declarator) -> String {
        match declarator {
            ast::Declarator::Identifier(_, name) => name.clone(),
            ast::Declarator::Function(_, inner, _) => self.declarator_name(inner),
        }
    }

    fn lower_compound_stmt(
        &mut self,
        stmt: &ast::CompoundStmt,
        create_scope: bool,
    ) -> Result<tir::CompoundStmt> {
        if create_scope {
            self.scope_tree.enter();
        }

        let mut items = Vec::new();

        for item in &stmt.items {
            match item {
                ast::BlockItem::Declaration(decl) => {
                    let decl = self.lower_declaration(decl)?;

                    items.push(tir::BlockItem::Declaration(decl));
                }

                ast::BlockItem::Statement(stmt) => {
                    let stmt = self.lower_stmt(stmt)?;

                    items.push(tir::BlockItem::Statement(stmt));
                }
            }
        }

        if create_scope {
            self.scope_tree.exit();
        }

        Ok(tir::CompoundStmt {
            span: stmt.span,
            items,
        })
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Result<tir::Stmt> {
        match stmt {
            ast::Stmt::Compound(block) => {
                Ok(tir::Stmt::Compound(self.lower_compound_stmt(block, true)?))
            }

            ast::Stmt::Return(span, expr) => self.lower_return(*span, expr.as_ref()),

            ast::Stmt::Expression(span, expr) => Ok(tir::Stmt::Expression {
                span: *span,
                expr: self.lower_expr(*expr.clone())?,
            }),
        }
    }

    fn lower_return(&mut self, span: Span, expr: Option<&ast::Expr>) -> Result<tir::Stmt> {
        let return_type = self.current_function.as_ref().unwrap().return_type.clone();

        let expr = if let Some(expr) = expr {
            let expr = self.lower_expr(expr.clone())?;

            if !return_type.type_compatible(&expr.qtype) {
                return Err(self.error(
                    span,
                    ErrorKind::ReturnTypeMismatch(return_type.clone(), expr.qtype),
                ));
            }

            Some(self.fold_expr(expr.cast_to(return_type.clone()))?)
        } else {
            let typ = QualType::new(Type::Primitive(Primitive::Void));

            if !return_type.type_compatible(&typ) {
                return Err(self.error(
                    span,
                    ErrorKind::ReturnTypeMismatch(return_type.clone(), typ),
                ));
            }

            None
        };

        Ok(tir::Stmt::Return { span, expr })
    }
}
