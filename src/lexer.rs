use crate::token::{Span, Token, TokenKind};

pub struct Lexer<'a> {
    pub source: &'a str,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Lexer {
            source,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn current(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.current()?;

        self.pos += 1;

        if ch == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }

        Some(ch)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while let Some(ch) = self.current() {
                if ch.is_ascii_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            if self.current() == Some(b'/') && self.peek() == Some(b'/') {
                self.advance();
                self.advance();

                while let Some(ch) = self.current() {
                    if ch == b'\n' {
                        break;
                    }
                    self.advance();
                }

                continue;
            }

            if self.current() == Some(b'/') && self.peek() == Some(b'*') {
                self.advance();
                self.advance();

                while let Some(ch) = self.current() {
                    if ch == b'*' && self.peek() == Some(b'/') {
                        self.advance();
                        self.advance();
                        break;
                    }

                    self.advance();
                }

                continue;
            }

            break;
        }
    }

    fn make_token(
        &self,
        kind: TokenKind,
        literal: &str,
        start: usize,
        line: usize,
        col: usize,
    ) -> Token {
        Token {
            kind,
            literal: literal.to_string(),
            span: Span {
                start,
                end: self.pos,
            },
            line,
            col,
        }
    }

    fn scan_number(&mut self, start: usize, line: usize, col: usize) -> Token {
        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        let literal = &self.source[start..self.pos];
        self.make_token(TokenKind::IntLit, literal, start, line, col)
    }

    fn scan_identifier(&mut self, start: usize, line: usize, col: usize) -> Token {
        while let Some(ch) = self.current() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                self.advance();
            } else {
                break;
            }
        }

        let literal = &self.source[start..self.pos];
        let kind = match literal {
            "int" => TokenKind::Int,
            "void" => TokenKind::Void,
            "return" => TokenKind::Return,
            _ => TokenKind::Identifier,
        };

        self.make_token(kind, literal, start, line, col)
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        let start = self.pos;
        let col = self.col;
        let line = self.line;

        match self.current() {
            None => self.make_token(TokenKind::Eof, "", start, line, col),
            Some(ch) if ch.is_ascii_digit() => self.scan_number(start, line, col),
            Some(ch) if ch.is_ascii_alphabetic() || ch == b'_' => {
                self.scan_identifier(start, line, col)
            }
            Some(b'+') => {
                self.advance();
                self.make_token(TokenKind::Plus, "+", start, line, col)
            }
            Some(b'-') => {
                self.advance();
                self.make_token(TokenKind::Minus, "-", start, line, col)
            }
            Some(b'*') => {
                self.advance();
                self.make_token(TokenKind::Star, "*", start, line, col)
            }
            Some(b'/') => {
                self.advance();
                self.make_token(TokenKind::Slash, "/", start, line, col)
            }
            Some(b'(') => {
                self.advance();
                self.make_token(TokenKind::LParen, "(", start, line, col)
            }
            Some(b')') => {
                self.advance();
                self.make_token(TokenKind::RParen, ")", start, line, col)
            }
            Some(b'{') => {
                self.advance();
                self.make_token(TokenKind::LBrace, "{}", start, line, col)
            }
            Some(b'}') => {
                self.advance();
                self.make_token(TokenKind::RBrace, "}", start, line, col)
            }
            Some(b';') => {
                self.advance();
                self.make_token(TokenKind::Semi, ";", start, line, col)
            }
            Some(b'=') => {
                self.advance();
                self.make_token(TokenKind::Equals, "=", start, line, col)
            }
            Some(b',') => {
                self.advance();
                self.make_token(TokenKind::Comma, ",", start, line, col)
            }
            Some(_) => {
                self.advance();
                self.make_token(
                    TokenKind::Invalid,
                    &self.source[start..self.pos],
                    start,
                    line,
                    col,
                )
            }
        }
    }
}
