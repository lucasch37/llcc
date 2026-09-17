use crate::{
    semantic::{
        env::Symbol,
        types::{Primitive, QualType, Type},
    },
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    Double(f64),
    Char(i8),
}

impl Value {
    pub fn cast_to(self, typ: &Type) -> Option<Self> {
        match typ {
            Type::Primitive(Primitive::Int) => Some(Value::Int(match self {
                Value::Char(v) => v as i32,
                Value::Int(v) => v,
                Value::Float(v) => v as i32,
                Value::Double(v) => v as i32,
                _ => return None,
            })),

            Type::Primitive(Primitive::Float) => Some(Value::Float(match self {
                Value::Char(v) => v as f32,
                Value::Int(v) => v as f32,
                Value::Float(v) => v,
                Value::Double(v) => v as f32,
                _ => return None,
            })),

            Type::Primitive(Primitive::Double) => Some(Value::Double(match self {
                Value::Char(v) => v as f64,
                Value::Int(v) => v as f64,
                Value::Float(v) => v as f64,
                Value::Double(v) => v,
                _ => return None,
            })),

            Type::Primitive(Primitive::Char) => Some(Value::Char(match self {
                Value::Char(v) => v,
                Value::Int(v) => v as i8,
                Value::Float(v) => v as i8,
                Value::Double(v) => v as i8,
            })),

            _ => None,
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
    Value(Value),
    Symbol(Symbol),
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
