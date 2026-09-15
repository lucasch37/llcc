use crate::{
    semantic::{env::Symbol, types::QualType},
    token::Span,
};

#[derive(Debug, Clone)]
pub struct TranslationUnit {
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
    pub symbol: Symbol,
    pub qtype: QualType,
    pub params: Vec<Parameter>,
    pub body: CompoundStmt,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub span: Span,
    pub symbol: Symbol,
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
    pub symbol: Symbol,
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

#[derive(Debug, Clone)]
pub struct Expr {
    pub span: Span,
    pub qtype: QualType,
    pub kind: ExprKind,
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
    IntLit(i32),
    Symbol(Symbol),
    Assignment {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
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
