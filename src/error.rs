use std::{fmt, result};

use crate::{semantic::types::QualType, token::Span};

#[derive(Debug, PartialEq, Clone)]
pub enum ErrorKind {
    // parsing errors
    ExpectedFound(String, String),
    InvalidAssignmentTarget,

    // semantic errors
    Redefinition(String),
    UndeclaredIdent(String),
    ReturnTypeMismatch(QualType, QualType),
    TypeMismatch(QualType, QualType),
    InitializedFunction,
    NotConstInitializer,

    // folding errors
    IntegerOverflow(QualType),
    DivisionByZero,

    MultipleErrors(Vec<Error>),
}

impl ErrorKind {
    pub fn message(&self) -> String {
        match self {
            ErrorKind::ExpectedFound(expected, found) => {
                format!("Expected {}, found '{}'", expected, found)
            }
            ErrorKind::InvalidAssignmentTarget => "Invalid assignment target".to_string(),
            ErrorKind::Redefinition(name) => {
                format!("Redefinition of '{}'", name)
            }
            ErrorKind::UndeclaredIdent(name) => {
                format!("Undeclared identifier '{}'", name)
            }
            ErrorKind::ReturnTypeMismatch(expected, found) => {
                format!(
                    "Return type mismatch: expected '{}', found '{}'",
                    expected, found
                )
            }
            ErrorKind::TypeMismatch(expected, found) => {
                format!("Type mismatch: expected '{}', found '{}'", expected, found)
            }
            ErrorKind::InitializedFunction => "Only variables can be initialized".to_string(),
            ErrorKind::NotConstInitializer => {
                "Initializer element is not a compile-time constant".to_string()
            }
            ErrorKind::IntegerOverflow(ty) => {
                format!("Integer overflow for type '{}'", ty)
            }
            ErrorKind::DivisionByZero => "Division by zero".to_string(),
            ErrorKind::MultipleErrors(errors) => {
                unreachable!("MultipleErrors should be handled separately and not printed directly")
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Error {
    pub source: String,
    pub line: usize,
    pub col: usize,
    pub filename: String,
    pub span: Span,
    pub kind: ErrorKind,
}

pub type Result<T> = result::Result<T, Error>;

impl Error {
    pub fn new(source: String, filename: String, span: Span, kind: ErrorKind) -> Self {
        let (line, col) = Self::get_line_col(&source, span);

        Self {
            source,
            line,
            col,
            filename,
            span,
            kind,
        }
    }

    pub fn new_multiple(errors: Vec<Error>) -> Self {
        let source = errors.get(0).map(|e| e.source.clone()).unwrap_or_default();
        let filename = errors
            .get(0)
            .map(|e| e.filename.clone())
            .unwrap_or_default();

        Self {
            source,
            line: 0,
            col: 0,
            filename,
            span: Span { start: 0, end: 0 },
            kind: ErrorKind::MultipleErrors(errors),
        }
    }

    fn get_line_col(source: &str, span: Span) -> (usize, usize) {
        let before = &source[..span.start];

        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;

        let column = before
            .rfind('\n')
            .map_or(span.start, |last_newline| span.start - last_newline - 1)
            + 1;

        (line, column)
    }

    pub fn print_error(&self) {
        if let ErrorKind::MultipleErrors(errors) = self.kind.clone() {
            for error in errors {
                error.print_error();
            }
            return;
        }

        const RED: &str = "\x1b[31m";
        const BLUE: &str = "\x1b[34m";
        const BOLD: &str = "\x1b[1m";
        const GREY: &str = "\x1b[90m";
        const UNDERLINE: &str = "\x1b[4m";
        const RESET: &str = "\x1b[0m";

        let start = self.span.start.min(self.source.len());
        let end = self.span.end.min(self.source.len());

        let line_start = self.source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);

        let line_end = self.source[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(self.source.len());

        let source_line = &self.source[line_start..line_end];

        let highlight_len = if end > start {
            (end.min(line_end) - start).max(1)
        } else {
            1
        };

        let line_count = self.source.lines().count();

        let line_num_width = line_count.to_string().len();

        eprintln!(
            "{:>width$} {BOLD}{RED}× error:{RESET} {}",
            "",
            self.kind.message(),
            width = line_num_width,
        );

        eprintln!(
            "{:width$} {BLUE}╭─▶ {RESET}[{UNDERLINE}{}:{}:{}{RESET}]",
            "",
            self.filename,
            self.line,
            self.col,
            width = line_num_width
        );

        eprintln!("{BLUE}{:>width$} │{RESET}", "", width = line_num_width);

        eprintln!(
            "{GREY}{:>width$} {BLUE}│{RESET} {}",
            self.line,
            source_line,
            width = line_num_width
        );

        eprintln!(
            "{BLUE}{:>width$} │{RESET} {}{RED}{}{RESET}",
            "",
            " ".repeat(start - line_start),
            "^".repeat(highlight_len),
            width = line_num_width
        );

        eprintln!("{BLUE}{:>width$} ╰──", "", width = line_num_width);
    }
}
