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
        let qtype = self.parse_decl_specifiers(&decl.specifiers);
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

    pub fn parse_decl_specifiers(&self, specifiers: &ast::DeclSpecifiers) -> QualType {
        let typ = match specifiers.type_specifiers[0] {
            ast::TypeSpecifier::Void => Type::Primitive(Primitive::Void),
            ast::TypeSpecifier::Int => Type::Primitive(Primitive::Int),
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
        let (name, qtype) = self.lower_declarator_type(declarator, base_qtype);

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
                    self.lower_expr(&expr).map(tir::Initializer::Expr)
                }
            })
            .transpose()?;

        if let Some(tir::Initializer::Expr(expr)) = initializer.as_ref() {
            if !qtype.type_compatible(&expr.qtype) {
                return Err(self.error(
                    span,
                    ErrorKind::TypeMismatch((qtype.clone()), (expr.qtype.clone())),
                ));
            }
            if qtype.typ.is_function() {
                return Err(self.error(span, ErrorKind::InitializedFunction));
            }
        }

        if self.scope_tree.is_global() {
            if let Some(tir::Initializer::Expr(expr)) = initializer.as_mut() {
                let value = self.fold_global_constant(expr)?;
                expr.kind = tir::ExprKind::IntLit(value);
            }
        }

        Ok(tir::InitDeclarator {
            span,
            symbol,
            qtype,
            initializer,
        })
    }

    fn fold_global_constant(&mut self, expr: &tir::Expr) -> Result<i32> {
        match &expr.kind {
            tir::ExprKind::IntLit(value) => Ok(*value),
            tir::ExprKind::Unary { op, expr } => {
                let value = self.fold_global_constant(expr)?;
                match op {
                    tir::UnaryOp::Pos => Ok(value),
                    tir::UnaryOp::Neg => value.checked_neg().ok_or(self.error(
                        expr.span,
                        ErrorKind::IntegerOverflow(
                            QualType::new(Type::Primitive(Primitive::Int)).into(),
                        ),
                    )),
                }
            }
            tir::ExprKind::Binary { lhs, op, rhs } => {
                let lhs = self.fold_global_constant(lhs)?;
                let rhs = self.fold_global_constant(rhs)?;
                match op {
                    tir::BinaryOp::Add => lhs.checked_add(rhs),
                    tir::BinaryOp::Sub => lhs.checked_sub(rhs),
                    tir::BinaryOp::Mul => lhs.checked_mul(rhs),
                    tir::BinaryOp::Div => {
                        if rhs == 0 {
                            return Err(self.error(expr.span, ErrorKind::DivisionByZero));
                        }
                        lhs.checked_div(rhs)
                    }
                }
                .ok_or(self.error(
                    expr.span,
                    ErrorKind::IntegerOverflow(
                        QualType::new(Type::Primitive(Primitive::Int)).into(),
                    ),
                ))
            }
            _ => Err(self.error(expr.span, ErrorKind::NotConstInitializer)),
        }
    }

    fn lower_declarator_type(
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
                        self.lower_declarator_type(
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

                self.lower_declarator_type(*inner, function_qtype)
            }
        }
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> Result<tir::Expr> {
        let span = *expr.span();
        let (qtype, kind) = match expr {
            ast::Expr::IntLit(_, literal) => (
                QualType::new(Type::Primitive(Primitive::Int)),
                tir::ExprKind::IntLit(literal.parse().expect("validated integer literal")),
            ),
            ast::Expr::Identifier(_, name) => {
                let Some(symbol) = self.scope_tree.lookup(&name) else {
                    return Err(self.error(span, ErrorKind::UndeclaredIdent(name.clone())));
                };

                (symbol.qtype.clone(), tir::ExprKind::Symbol(symbol.clone()))
            }
            ast::Expr::Binary(_, lhs, op, rhs) => {
                let lhs = Box::new(self.lower_expr(lhs)?);
                let rhs = Box::new(self.lower_expr(rhs)?);
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

                (
                    lhs.qtype.clone(),
                    tir::ExprKind::Assignment {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                )
            }
        };

        Ok(tir::Expr { span, qtype, kind })
    }

    pub fn lower_function_def(
        &mut self,
        function_def: &ast::FunctionDef,
    ) -> Result<tir::FunctionDef> {
        let return_type = self.parse_decl_specifiers(&function_def.specifiers);
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

            Some(expr)
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
