use crate::token::Span;

pub mod pretty_printer;

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
pub enum TypeSpecifier {
    Void,
    Int,
    Float,
    Double,
    Char,
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub span: Span,
    pub specifiers: DeclSpecifiers,
    pub declarator: Declarator,
    pub body: CompoundStmt,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub span: Span,
    pub specifiers: DeclSpecifiers,
    pub declarators: Vec<InitDeclarator>,
}

#[derive(Debug, Clone)]
pub struct InitDeclarator {
    pub span: Span,
    pub declarator: Declarator,
    pub initializer: Option<Initializer>,
}

#[derive(Debug, Clone)]
pub enum Initializer {
    Expr(Span, Expr),
    // TODO: for arrays
    // List {
    //     span: Span,
    //     elements: Vec<Initializer>,
    // },
}

impl Initializer {
    pub fn span(&self) -> &Span {
        match self {
            Initializer::Expr(span, ..) => span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeclSpecifiers {
    pub span: Span,
    pub type_specifiers: Vec<TypeSpecifier>,
}

#[derive(Debug, Clone)]
pub enum Declarator {
    Identifier(Span, String),
    Function(Span, Box<Declarator>, Vec<Declarator>),
}

impl Declarator {
    pub fn span(&self) -> &Span {
        match self {
            Declarator::Identifier(span, ..) | Declarator::Function(span, ..) => span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Compound(CompoundStmt),
    Return(Span, Option<Expr>),
    Expression(Span, Box<Expr>),
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
pub enum FloatSuffix {
    None,
    Float,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Binary(Span, Box<Expr>, BinaryOp, Box<Expr>),
    Unary(Span, UnaryOp, Box<Expr>),
    IntLit(Span, String),
    FloatLit(Span, String, FloatSuffix),
    CharLit(Span, String),
    Identifier(Span, String),
    Assignment(Span, Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn span(&self) -> &Span {
        match self {
            Expr::Binary(span, ..)
            | Expr::Unary(span, ..)
            | Expr::IntLit(span, ..)
            | Expr::FloatLit(span, ..)
            | Expr::CharLit(span, ..)
            | Expr::Assignment(span, ..)
            | Expr::Identifier(span, ..) => span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Pos,
    Neg,
}
