use crate::{
    semantic::{
        env::{SymbolId, SymbolTable},
        types::{Primitive, QualType, Type},
    },
    token::Span,
};

#[derive(Debug, Clone)]
pub struct TranslationUnit {
    pub symbols: SymbolTable,
    pub external_declarations: Vec<ExternalDeclaration>,
}

#[derive(Debug, Clone)]
pub enum ExternalDeclaration {
    FunctionDef(FunctionDef),
    Declaration(Declaration),
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub qtype: QualType,
    pub params: Vec<Parameter>,
    pub body: CompoundStmt,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub qtype: QualType,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub span: Span,
    pub declarators: Vec<InitDeclarator>,
}

#[derive(Debug, Clone)]
pub struct InitDeclarator {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub qtype: QualType,
    pub initializer: Option<Initializer>,
}

#[derive(Debug, Clone)]
pub enum Initializer {
    Expr(Expr),
}

impl Initializer {
    pub fn span(&self) -> &Span {
        match self {
            Initializer::Expr(expr) => &expr.span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Compound(CompoundStmt),
    Return { span: Span, expr: Option<Expr> },
    Expression { span: Span, expr: Expr },
}

#[derive(Debug, Clone)]
pub struct CompoundStmt {
    pub span: Span,
    pub items: Vec<BlockItem>,
}

#[derive(Debug, Clone)]
pub enum BlockItem {
    Declaration(Declaration),
    Statement(Stmt),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    LValue,
    RValue,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub span: Span,
    pub qtype: QualType,
    pub kind: ExprKind,
    pub value_kind: ValueKind,
}

impl Expr {
    pub fn int_promote(self) -> Expr {
        if self.qtype.typ.get_primitive().is_none() || self.qtype.typ.is_void() {
            return self;
        }

        // chars become ints
        if self.qtype.typ.size() < Type::Primitive(Primitive::Int).size() {
            self.cast_to(QualType::new(Type::Primitive(Primitive::Int)))
        } else {
            self
        }
    }

    pub fn cast_to(self, new_type: QualType) -> Self {
        Self {
            qtype: new_type.clone(),
            value_kind: ValueKind::RValue,
            span: self.span.clone(),
            kind: ExprKind::Cast {
                new_type,
                expr: Box::new(self),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Binary {
        lhs: Box<Expr>,
        op: BinaryOp,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Literal(LiteralKind),
    Symbol(SymbolId),
    Assignment {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Cast {
        new_type: QualType,
        expr: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Pos,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiteralKind {
    Int(i32),
    Float(f32),
    Double(f64),
    Char(u8),
}
