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

        let mut symbol = self
            .scope_tree
            .new_symbol(name.clone(), qtype.clone(), kind);

        if let Some(existing) = self.scope_tree.lookup_local(&name) {
            if self.scope_tree.check_redefinition(&symbol, existing) {
                return Err(self.error(span, ErrorKind::Redefinition(name)));
            }
            if self.scope_tree.is_global() {
                symbol.id = existing.id;
                if existing.kind == env::InitType::Definition {
                    symbol.kind = env::InitType::Definition;
                }
            }
        }

        self.scope_tree.define(name, symbol.clone());

        let mut initializer = initializer
            .map(|initializer| match initializer {
                ast::Initializer::Expr(_, expr) => {
                    let expr = self.lower_expr(&expr)?;
                    let expr = self.implicit_cast(expr, qtype.clone())?;

                    Ok(tir::Initializer::Expr(expr))
                }
            })
            .transpose()?;

        if let Some(tir::Initializer::Expr(expr)) = initializer.as_ref() {
            if qtype.typ.is_function() {
                return Err(self.error(span, ErrorKind::InitializedFunction));
            }
        }

        if self.scope_tree.is_global() {
            if let Some(tir::Initializer::Expr(expr)) = initializer.as_mut() {
                let value = self.fold_global_variable(expr)?;
                expr.kind = tir::ExprKind::Value(value);
            }
        }

        Ok(tir::InitDeclarator {
            span,
            symbol,
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

    fn fold_global_variable(&mut self, expr: &tir::Expr) -> Result<tir::Value> {
        match &expr.kind {
            tir::ExprKind::Value(value) => Ok(*value),
            tir::ExprKind::Cast { new_type, expr } => {
                let span = expr.span;
                let value = self.fold_global_variable(expr)?;

                value
                    .cast_to(&new_type.typ)
                    .ok_or_else(|| self.error(span, ErrorKind::NotConstInitializer))
            }
            tir::ExprKind::Unary { op, expr } => {
                let value = self.fold_global_variable(expr)?;
                match op {
                    tir::UnaryOp::Pos => Ok(value),
                    tir::UnaryOp::Neg => match value {
                        tir::Value::Int(value) => {
                            value.checked_neg().map(tir::Value::Int).ok_or(self.error(
                                expr.span,
                                ErrorKind::IntegerOverflow(
                                    QualType::new(Type::Primitive(Primitive::Int)).into(),
                                ),
                            ))
                        }
                        tir::Value::Char(value) => Ok(tir::Value::Char(-value)),
                        tir::Value::Float(value) => Ok(tir::Value::Float(-value)),
                        tir::Value::Double(value) => Ok(tir::Value::Double(-value)),
                    },
                }
            }
            tir::ExprKind::Binary { lhs, op, rhs } => {
                let lhs = self.fold_global_variable(lhs)?;
                let rhs = self.fold_global_variable(rhs)?;
                match (lhs, rhs) {
                    (tir::Value::Int(lhs), tir::Value::Int(rhs)) => {
                        let value = match op {
                            tir::BinaryOp::Add => lhs.checked_add(rhs),
                            tir::BinaryOp::Sub => lhs.checked_sub(rhs),
                            tir::BinaryOp::Mul => lhs.checked_mul(rhs),
                            tir::BinaryOp::Div if rhs != 0 => lhs.checked_div(rhs),
                            tir::BinaryOp::Div => {
                                return Err(self.error(expr.span, ErrorKind::DivisionByZero));
                            }
                        };
                        value.map(tir::Value::Int).ok_or(self.error(
                            expr.span,
                            ErrorKind::IntegerOverflow(
                                QualType::new(Type::Primitive(Primitive::Int)).into(),
                            ),
                        ))
                    }
                    (tir::Value::Float(lhs), tir::Value::Float(rhs)) => {
                        Ok(tir::Value::Float(match op {
                            tir::BinaryOp::Add => lhs + rhs,
                            tir::BinaryOp::Sub => lhs - rhs,
                            tir::BinaryOp::Mul => lhs * rhs,
                            tir::BinaryOp::Div => lhs / rhs,
                        }))
                    }
                    (tir::Value::Double(lhs), tir::Value::Double(rhs)) => {
                        Ok(tir::Value::Double(match op {
                            tir::BinaryOp::Add => lhs + rhs,
                            tir::BinaryOp::Sub => lhs - rhs,
                            tir::BinaryOp::Mul => lhs * rhs,
                            tir::BinaryOp::Div => lhs / rhs,
                        }))
                    }
                    _ => unreachable!("arithmetic operands have a common type"),
                }
            }
            _ => Err(self.error(expr.span, ErrorKind::NotConstInitializer)),
        }
    }

    fn implicit_cast(&mut self, expr: tir::Expr, target: QualType) -> Result<tir::Expr> {
        if expr.qtype == target {
            return Ok(expr);
        }

        if !expr.qtype.type_compatible(&target) {
            return Err(self.error(
                expr.span,
                ErrorKind::TypeMismatch(expr.qtype.clone(), target.clone()),
            ));
        }

        Ok(tir::Expr {
            span: expr.span,
            qtype: target.clone(),
            kind: tir::ExprKind::Cast {
                new_type: target.clone(),
                expr: Box::new(expr),
            },
        })
    }

    fn resolve_arithmetic_type(lhs: &QualType, rhs: &QualType) -> QualType {
        if matches!(lhs.typ, Type::Primitive(Primitive::Double))
            || matches!(rhs.typ, Type::Primitive(Primitive::Double))
        {
            QualType::new(Type::Primitive(Primitive::Double))
        } else if matches!(lhs.typ, Type::Primitive(Primitive::Float))
            || matches!(rhs.typ, Type::Primitive(Primitive::Float))
        {
            QualType::new(Type::Primitive(Primitive::Float))
        } else {
            QualType::new(Type::Primitive(Primitive::Int))
        }
    }

    fn decode_char_literal(literal: &str) -> i8 {
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

        value as i8
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> Result<tir::Expr> {
        let span = *expr.span();
        let (qtype, kind) = match expr {
            ast::Expr::IntLit(_, literal) => (
                QualType::new(Type::Primitive(Primitive::Int)),
                tir::ExprKind::Value(tir::Value::Int(
                    literal.parse().expect("validated integer literal"),
                )),
            ),
            ast::Expr::FloatLit(_, literal, suffix) => match suffix {
                ast::FloatSuffix::None => (
                    QualType::new(Type::Primitive(Primitive::Double)),
                    tir::ExprKind::Value(tir::Value::Double(
                        literal.parse().expect("validated float literal"),
                    )),
                ),
                ast::FloatSuffix::Float => (
                    QualType::new(Type::Primitive(Primitive::Float)),
                    tir::ExprKind::Value(tir::Value::Float(
                        literal.parse().expect("validated float literal"),
                    )),
                ),
            },
            ast::Expr::CharLit(_, literal) => (
                QualType::new(Type::Primitive(Primitive::Char)),
                tir::ExprKind::Value(tir::Value::Char(Self::decode_char_literal(literal))),
            ),
            ast::Expr::Identifier(_, name) => {
                let Some(symbol) = self.scope_tree.lookup(&name) else {
                    return Err(self.error(span, ErrorKind::UndeclaredIdent(name.clone())));
                };

                (symbol.qtype.clone(), tir::ExprKind::Symbol(symbol.clone()))
            }
            ast::Expr::Binary(_, lhs, op, rhs) => {
                let lhs = self.lower_expr(lhs)?;
                let rhs = self.lower_expr(rhs)?;

                let result_type = Self::resolve_arithmetic_type(&lhs.qtype, &rhs.qtype);

                let lhs = Box::new(self.implicit_cast(lhs, result_type.clone())?);
                let rhs = Box::new(self.implicit_cast(rhs, result_type.clone())?);

                let op = match op {
                    ast::BinaryOp::Add => tir::BinaryOp::Add,
                    ast::BinaryOp::Sub => tir::BinaryOp::Sub,
                    ast::BinaryOp::Mul => tir::BinaryOp::Mul,
                    ast::BinaryOp::Div => tir::BinaryOp::Div,
                };

                (lhs.qtype.clone(), tir::ExprKind::Binary { lhs, op, rhs })
            }
            ast::Expr::Unary(_, op, expr) => {
                let expr = Box::new(self.lower_expr(expr)?);
                let qtype = expr.qtype.clone();
                let op = match op {
                    ast::UnaryOp::Pos => tir::UnaryOp::Pos,
                    ast::UnaryOp::Neg => tir::UnaryOp::Neg,
                };
                (qtype, tir::ExprKind::Unary { op, expr })
            }
            ast::Expr::Assignment(_, lhs, rhs) => {
                let lhs = self.lower_expr(lhs)?;
                let rhs = self.lower_expr(rhs)?;

                if !matches!(lhs.kind, tir::ExprKind::Symbol(_)) {
                    Err(self.error(lhs.span, ErrorKind::InvalidAssignmentTarget))?;
                }

                if !lhs.qtype.type_compatible(&rhs.qtype) {
                    Err(self.error(
                        span,
                        ErrorKind::TypeMismatch(lhs.qtype.clone(), rhs.qtype.clone()),
                    ))?;
                }

                let lhs = Box::new(self.implicit_cast(lhs.clone(), lhs.qtype.clone())?);
                let rhs = Box::new(self.implicit_cast(rhs.clone(), lhs.qtype.clone())?);

                (lhs.qtype.clone(), tir::ExprKind::Assignment { lhs, rhs })
            }
        };

        Ok(tir::Expr { span, qtype, kind })
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

        let symbol = self.scope_tree.new_symbol(
            name.clone(),
            QualType::new(Type::Function(func_type.clone())),
            env::InitType::Definition,
        );

        match self.scope_tree.lookup(&name) {
            Some(existing) => {
                if self.scope_tree.check_redefinition(&symbol, existing) {
                    return Err(self.error(function_def.span, ErrorKind::Redefinition(name)));
                }
            }
            None => {}
        };

        self.current_function = Some(FunctionContext {
            return_type: func_type.return_type.as_ref().clone(),
        });

        self.scope_tree.define(name, symbol.clone());

        self.scope_tree.enter();

        let compound_stmt = self.lower_compound_stmt(&function_def.body, false)?;

        self.scope_tree.exit();

        Ok(tir::FunctionDef {
            span: function_def.span,
            symbol,
            qtype: QualType::new(Type::Function(func_type.clone())),
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
                expr: self.lower_expr(expr.as_ref())?,
            }),
        }
    }

    fn lower_return(&mut self, span: Span, expr: Option<&ast::Expr>) -> Result<tir::Stmt> {
        let return_type = self.current_function.as_ref().unwrap().return_type.clone();

        let expr = if let Some(expr) = expr {
            let expr = self.lower_expr(expr)?;

            if !return_type.type_compatible(&expr.qtype) {
                return Err(self.error(
                    span,
                    ErrorKind::ReturnTypeMismatch(return_type.clone(), expr.qtype),
                ));
            }

            Some(self.implicit_cast(expr, return_type.clone())?)
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
