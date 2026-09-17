use std::result;

use crate::{
    ast,
    error::{Error, ErrorKind, Result},
    lexer::Lexer,
    token::{Span, Token, TokenKind},
};

pub struct Parser<'a> {
    filename: &'a str,
    scanner: Lexer<'a>,
    current: Token,
    next: Token,
}

impl<'a> Parser<'a> {
    pub fn new(mut scanner: Lexer<'a>, filename: &'a str) -> Self {
        let current = scanner.next_token();
        let next = scanner.next_token();

        Self {
            filename,
            scanner,
            current,
            next,
        }
    }

    fn advance(&mut self) {
        self.current = std::mem::replace(&mut self.next, self.scanner.next_token());
    }

    fn eat(&mut self, kind: TokenKind) -> Result<()> {
        if self.current.kind == kind {
            self.advance();
            Ok(())
        } else {
            Err(Error::new(
                self.scanner.source.to_string(),
                self.filename.to_string(),
                self.current.span,
                ErrorKind::ExpectedFound(
                    kind.lexeme().to_string(),
                    self.current.literal.to_string(),
                ),
            ))
        }
    }

    // translation_unit = external_declaration*
    pub fn parse_translation_unit(&mut self) -> Result<ast::TranslationUnit> {
        let mut external_declarations = Vec::new();

        while self.current.kind != TokenKind::Eof {
            external_declarations.push(self.parse_external_declaration()?);
        }

        Ok(ast::TranslationUnit {
            external_declarations,
        })
    }

    // external_declaration = function_def | declaration
    fn parse_external_declaration(&mut self) -> Result<ast::ExternalDeclaration> {
        let start = self.current.span.start;

        let specifiers = self.parse_decl_specifiers()?;
        let declarator = self.parse_declarator()?;

        if self.current.kind == TokenKind::LBrace {
            return Ok(ast::ExternalDeclaration::FunctionDef(
                self.parse_function_def(start, specifiers, declarator)?,
            ));
        }

        let declaration = self.parse_declaration(start, specifiers, Some(declarator))?;

        Ok(ast::ExternalDeclaration::Declaration(declaration))
    }

    // function_def = decl_specifiers declarator compound_stmt
    fn parse_function_def(
        &mut self,
        start: usize,
        specifiers: ast::DeclSpecifiers,
        declarator: ast::Declarator,
    ) -> Result<ast::FunctionDef> {
        let body = self.parse_compound_stmt()?;

        Ok(ast::FunctionDef {
            span: Span {
                start,
                end: body.span.end,
            },
            specifiers,
            declarator,
            body,
        })
    }

    // declaration = decl_specifiers init_declarator ("," init_declarator)* ";"
    fn parse_declaration(
        &mut self,
        start: usize,
        specifiers: ast::DeclSpecifiers,
        first_declarator: Option<ast::Declarator>,
    ) -> Result<ast::Declaration> {
        let first_declarator = match first_declarator {
            Some(declarator) => declarator,
            None => self.parse_declarator()?,
        };
        let mut declarators = vec![self.parse_init_declarator(first_declarator)?];

        while self.current.kind == TokenKind::Comma {
            self.advance();
            let declarator = self.parse_declarator()?;
            declarators.push(self.parse_init_declarator(declarator)?);
        }

        let end = self.current.span.end;
        self.eat(TokenKind::Semi)?;

        Ok(ast::Declaration {
            span: Span { start, end },
            specifiers,
            declarators,
        })
    }

    // initializer = expr
    fn parse_initializer(&mut self) -> Result<ast::Initializer> {
        let expr = self.parse_expr()?;
        let span = Self::expr_span(&expr).clone();
        Ok(ast::Initializer::Expr(span, expr))
    }

    // init_declarator = declarator ("=" initializer)?
    fn parse_init_declarator(
        &mut self,
        declarator: ast::Declarator,
    ) -> Result<ast::InitDeclarator> {
        let start = declarator.span().start;

        let initializer = if self.current.kind == TokenKind::Equals {
            self.advance();
            Some(self.parse_initializer()?)
        } else {
            None
        };

        let end = match &initializer {
            Some(initializer) => initializer.span().end,
            None => declarator.span().end,
        };

        Ok(ast::InitDeclarator {
            span: Span { start, end },
            declarator,
            initializer,
        })
    }

    // decl_specifiers = type_specifier+
    fn parse_decl_specifiers(&mut self) -> Result<ast::DeclSpecifiers> {
        let start = self.current.span.start;
        let mut type_specifiers = Vec::new();

        while matches!(
            self.current.kind,
            TokenKind::Int
                | TokenKind::Void
                | TokenKind::Float
                | TokenKind::Double
                | TokenKind::Char
        ) {
            type_specifiers.push(self.parse_type_specifier()?);
        }

        let end = self.current.span.end;

        Ok(ast::DeclSpecifiers {
            span: Span { start, end },
            type_specifiers,
        })
    }

    // type_specifier = "int" | "void"
    fn parse_type_specifier(&mut self) -> Result<ast::TypeSpecifier> {
        let specifier = match self.current.kind {
            TokenKind::Int => ast::TypeSpecifier::Int,
            TokenKind::Float => ast::TypeSpecifier::Float,
            TokenKind::Double => ast::TypeSpecifier::Double,
            TokenKind::Char => ast::TypeSpecifier::Char,
            TokenKind::Void => ast::TypeSpecifier::Void,
            _ => {
                return Err(Error::new(
                    self.scanner.source.to_string(),
                    self.filename.to_string(),
                    self.current.span,
                    ErrorKind::ExpectedFound(
                        "type specifier".to_string(),
                        self.current.literal.to_string(),
                    ),
                ));
            }
        };

        self.advance();
        Ok(specifier)
    }

    // declarator = identifier | identifier "(" ")"
    fn parse_declarator(&mut self) -> Result<ast::Declarator> {
        let ident = self.current.clone();
        self.eat(TokenKind::Identifier)?;

        let mut declarator = ast::Declarator::Identifier(ident.span.clone(), ident.literal);

        if self.current.kind == TokenKind::LParen {
            let start = ident.span.start;
            self.advance();

            let end = self.current.span.end;
            self.eat(TokenKind::RParen)?;

            declarator =
                ast::Declarator::Function(Span { start, end }, Box::new(declarator), Vec::new());
        }

        Ok(declarator)
    }

    // stmt = return_stmt | compound_stmt | expr_stmt
    fn parse_stmt(&mut self) -> Result<ast::Stmt> {
        match self.current.kind {
            TokenKind::Return => self.parse_return_stmt(),
            TokenKind::LBrace => Ok(ast::Stmt::Compound(self.parse_compound_stmt()?)),
            TokenKind::Identifier
            | TokenKind::IntLit
            | TokenKind::FloatLit
            | TokenKind::CharLit => self.parse_expr_stmt(),
            _ => {
                return Err(Error::new(
                    self.scanner.source.to_string(),
                    self.filename.to_string(),
                    self.current.span,
                    ErrorKind::ExpectedFound(
                        "statement".to_string(),
                        self.current.literal.to_string(),
                    ),
                ));
            }
        }
    }

    // compound_stmt = "{" block_item_list "}"
    fn parse_compound_stmt(&mut self) -> Result<ast::CompoundStmt> {
        let start = self.current.span.start;
        self.eat(TokenKind::LBrace)?;

        let items = self.parse_block_item_list()?;

        let end = self.current.span.end;
        self.eat(TokenKind::RBrace)?;

        Ok(ast::CompoundStmt {
            span: Span { start, end },
            items,
        })
    }

    fn parse_block_item_list(&mut self) -> Result<Vec<ast::BlockItem>> {
        let mut items = Vec::new();

        while self.current.kind != TokenKind::RBrace {
            let item = if matches!(
                self.current.kind,
                TokenKind::Int
                    | TokenKind::Void
                    | TokenKind::Float
                    | TokenKind::Double
                    | TokenKind::Char
            ) {
                let start = self.current.span.start;
                let specifiers = self.parse_decl_specifiers()?;
                ast::BlockItem::Declaration(self.parse_declaration(start, specifiers, None)?)
            } else {
                ast::BlockItem::Statement(self.parse_stmt()?)
            };

            items.push(item);
        }

        Ok(items)
    }

    // expr_stmt = expr ";"
    fn parse_expr_stmt(&mut self) -> Result<ast::Stmt> {
        let start = self.current.span.start;
        let expr = self.parse_expr()?;
        let end = Self::expr_span(&expr).end;
        self.eat(TokenKind::Semi)?;

        Ok(ast::Stmt::Expression(Span { start, end }, Box::new(expr)))
    }

    // return_stmt = "return" expr? ";"
    fn parse_return_stmt(&mut self) -> Result<ast::Stmt> {
        let start = self.current.span.start;
        self.eat(TokenKind::Return)?;

        let expr = if self.current.kind == TokenKind::Semi {
            None
        } else {
            Some(self.parse_expr()?)
        };

        let end = self.current.span.end;
        self.eat(TokenKind::Semi)?;

        Ok(ast::Stmt::Return(Span { start, end }, expr))
    }

    fn expr_span(expr: &ast::Expr) -> &Span {
        match expr {
            ast::Expr::Binary(span, ..)
            | ast::Expr::Unary(span, ..)
            | ast::Expr::IntLit(span, ..)
            | ast::Expr::FloatLit(span, ..)
            | ast::Expr::CharLit(span, ..)
            | ast::Expr::Assignment(span, ..)
            | ast::Expr::Identifier(span, ..) => span,
        }
    }

    // primary_expr = IntLit | "(" expr ")"
    fn parse_primary_expr(&mut self) -> Result<ast::Expr> {
        match &self.current.kind {
            TokenKind::Identifier => {
                let span = self.current.span;
                let name = self.current.literal.clone();
                self.advance();
                Ok(ast::Expr::Identifier(span, name))
            }
            TokenKind::IntLit => {
                let span = self.current.span;
                let mut literal = self.current.literal.clone();
                self.advance();
                Ok(ast::Expr::IntLit(span, literal))
            }
            TokenKind::FloatLit => {
                let span = self.current.span;
                let mut literal = self.current.literal.clone();

                let mut suffix = ast::FloatSuffix::None;

                if literal.ends_with('f') || literal.ends_with('F') {
                    suffix = ast::FloatSuffix::Float;
                }

                if !matches!(suffix, ast::FloatSuffix::None) {
                    literal.pop();
                }

                self.advance();
                Ok(ast::Expr::FloatLit(span, literal, suffix))
            }
            TokenKind::CharLit => {
                let span = self.current.span;
                let literal = self.current.literal.clone();
                self.advance();
                Ok(ast::Expr::CharLit(span, literal))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.eat(TokenKind::RParen)?;

                Ok(expr)
            }
            _ => Err(Error::new(
                self.scanner.source.to_string(),
                self.filename.to_string(),
                self.current.span,
                ErrorKind::ExpectedFound(
                    "expression".to_string(),
                    self.current.literal.to_string(),
                ),
            )),
        }
    }

    // unary_expr = UnaryOp unary_expr | primary_expr
    fn parse_unary_expr(&mut self) -> Result<ast::Expr> {
        match &self.current.kind {
            TokenKind::Plus => {
                let start = self.current.span.start;
                self.advance();
                let expr = self.parse_unary_expr()?;
                let span = Span {
                    start,
                    end: Self::expr_span(&expr).end,
                };
                Ok(ast::Expr::Unary(span, ast::UnaryOp::Pos, Box::new(expr)))
            }
            TokenKind::Minus => {
                let start = self.current.span.start;
                self.advance();
                let expr = self.parse_unary_expr()?;
                let span = Span {
                    start,
                    end: Self::expr_span(&expr).end,
                };
                Ok(ast::Expr::Unary(span, ast::UnaryOp::Neg, Box::new(expr)))
            }
            _ => self.parse_primary_expr(),
        }
    }

    // multiplicative_expr = unary_expr (("*" | "/") unary_expr)*
    fn parse_multiplicative_expr(&mut self) -> Result<ast::Expr> {
        let mut expr = self.parse_unary_expr()?;

        while let TokenKind::Star | TokenKind::Slash = self.current.kind {
            let op = match self.current.kind {
                TokenKind::Star => ast::BinaryOp::Mul,
                TokenKind::Slash => ast::BinaryOp::Div,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span = Span {
                start: Self::expr_span(&expr).start,
                end: Self::expr_span(&right).end,
            };
            expr = ast::Expr::Binary(span, Box::new(expr), op, Box::new(right));
        }

        Ok(expr)
    }

    // additive_expr = multiplicative_expr (("+" | "-") multiplicative_expr)*
    fn parse_additive_expr(&mut self) -> Result<ast::Expr> {
        let mut expr = self.parse_multiplicative_expr()?;

        while let TokenKind::Plus | TokenKind::Minus = self.current.kind {
            let op = match self.current.kind {
                TokenKind::Plus => ast::BinaryOp::Add,
                TokenKind::Minus => ast::BinaryOp::Sub,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            let span = Span {
                start: Self::expr_span(&expr).start,
                end: Self::expr_span(&right).end,
            };
            expr = ast::Expr::Binary(span, Box::new(expr), op, Box::new(right));
        }

        Ok(expr)
    }

    // assignment_expr = additive_expr | unary_expr "=" assignment_expr
    fn parse_assignment_expr(&mut self) -> Result<ast::Expr> {
        let left = self.parse_additive_expr()?;

        if self.current.kind == TokenKind::Equals {
            let start = Self::expr_span(&left).start;
            self.advance();
            let right = self.parse_assignment_expr()?;
            let span = Span {
                start,
                end: Self::expr_span(&right).end,
            };
            Ok(ast::Expr::Assignment(span, Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    // expr = additive_expr
    pub fn parse_expr(&mut self) -> Result<ast::Expr> {
        self.parse_assignment_expr()
    }
}
