#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,
    Invalid,

    Int,
    Float,
    Double,
    Char,
    Void,
    Return,

    Identifier,
    IntLit,
    FloatLit,
    CharLit,
    Plus,
    Minus,
    Slash,
    Star,
    Equals,

    LParen,
    RParen,
    LBrace,
    RBrace,
    Semi,
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub literal: String,
    pub span: Span,
    pub line: usize,
    pub col: usize,
}

impl TokenKind {
    pub fn lexeme(&self) -> &'static str {
        match self {
            TokenKind::Eof => "EOF",
            TokenKind::Invalid => "Invalid",
            TokenKind::Int => "int",
            TokenKind::Float => "float",
            TokenKind::Double => "double",
            TokenKind::Char => "char",
            TokenKind::Void => "void",
            TokenKind::Return => "return",
            TokenKind::Identifier => "identifier",
            TokenKind::IntLit => "integer literal",
            TokenKind::FloatLit => "float literal",
            TokenKind::CharLit => "char literal",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Slash => "/",
            TokenKind::Star => "*",
            TokenKind::Equals => "=",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::Semi => ";",
            TokenKind::Comma => ",",
        }
    }
}
