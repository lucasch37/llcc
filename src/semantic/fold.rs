use crate::error::{Error, ErrorKind, Result};
use crate::semantic::SemanticAnalyzer;
use crate::semantic::tir::{BinaryOp, Expr, ExprKind, LiteralKind, UnaryOp, ValueKind};
use crate::semantic::types::{QualType, Type};
use crate::token::Span;

impl<'a> SemanticAnalyzer<'a> {
    pub fn fold_expr(&mut self, mut expr: Expr) -> Result<Expr> {
        expr.kind = self.fold_kind(expr.span, expr.kind)?;
        Ok(expr)
    }

    fn fold_kind(&mut self, span: Span, kind: ExprKind) -> Result<ExprKind> {
        match kind {
            ExprKind::Literal(_) => Ok(kind),

            ExprKind::Symbol(_) => {
                if self.scope_tree.is_global() {
                    return Err(self.error(span, ErrorKind::NotConstInitializer));
                }

                Ok(kind)
            }

            ExprKind::Binary { lhs, op, rhs } => {
                let lhs = self.fold_expr(*lhs)?;
                let rhs = self.fold_expr(*rhs)?;

                match (&lhs.kind, &rhs.kind) {
                    (ExprKind::Literal(left), ExprKind::Literal(right)) => {
                        let value = self.fold_binary(span, op, *left, *right)?;

                        Ok(ExprKind::Literal(value))
                    }

                    _ => Ok(ExprKind::Binary {
                        lhs: Box::new(lhs),
                        op,
                        rhs: Box::new(rhs),
                    }),
                }
            }

            ExprKind::Unary { op, expr } => {
                let expr = self.fold_expr(*expr)?;

                match expr.kind {
                    ExprKind::Literal(value) => {
                        Ok(ExprKind::Literal(self.fold_unary(span, op, value)?))
                    }

                    other => Ok(ExprKind::Unary {
                        op,
                        expr: Box::new(Expr {
                            kind: other,
                            ..expr
                        }),
                    }),
                }
            }

            ExprKind::Cast { new_type, expr } => {
                let expr = self.fold_expr(*expr)?;

                match expr.kind {
                    ExprKind::Literal(value) => {
                        Ok(ExprKind::Literal(self.fold_cast(value, &new_type)?))
                    }

                    other => Ok(ExprKind::Cast {
                        new_type,
                        expr: Box::new(Expr {
                            kind: other,
                            ..expr
                        }),
                    }),
                }
            }

            ExprKind::Assignment { lhs, rhs } => {
                let lhs = self.fold_expr(*lhs)?;
                let rhs = self.fold_expr(*rhs)?;

                Ok(ExprKind::Assignment {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                })
            }
        }
    }

    fn fold_binary(
        &mut self,
        span: Span,
        op: BinaryOp,
        lhs: LiteralKind,
        rhs: LiteralKind,
    ) -> Result<LiteralKind> {
        match (lhs, rhs) {
            (LiteralKind::Int(lhs), LiteralKind::Int(rhs)) => {
                let value = match op {
                    BinaryOp::Add => lhs.checked_add(rhs),
                    BinaryOp::Sub => lhs.checked_sub(rhs),
                    BinaryOp::Mul => lhs.checked_mul(rhs),
                    BinaryOp::Div => {
                        if rhs == 0 {
                            return Err(self.error(span, ErrorKind::DivisionByZero));
                        }

                        lhs.checked_div(rhs)
                    }
                };

                value.map(LiteralKind::Int).ok_or(self.error(
                    span,
                    ErrorKind::IntegerOverflow(QualType::new(Type::Primitive(
                        crate::semantic::types::Primitive::Int,
                    ))),
                ))
            }

            (LiteralKind::Float(lhs), LiteralKind::Float(rhs)) => {
                let value = match op {
                    BinaryOp::Add => lhs + rhs,
                    BinaryOp::Sub => lhs - rhs,
                    BinaryOp::Mul => lhs * rhs,
                    BinaryOp::Div => lhs / rhs,
                };

                Ok(LiteralKind::Float(value))
            }

            (LiteralKind::Double(lhs), LiteralKind::Double(rhs)) => {
                let value = match op {
                    BinaryOp::Add => lhs + rhs,
                    BinaryOp::Sub => lhs - rhs,
                    BinaryOp::Mul => lhs * rhs,
                    BinaryOp::Div => lhs / rhs,
                };

                Ok(LiteralKind::Double(value))
            }

            (LiteralKind::Char(lhs), LiteralKind::Char(rhs)) => {
                let lhs = lhs as i32;
                let rhs = rhs as i32;

                let value = match op {
                    BinaryOp::Add => lhs.checked_add(rhs),
                    BinaryOp::Sub => lhs.checked_sub(rhs),
                    BinaryOp::Mul => lhs.checked_mul(rhs),
                    BinaryOp::Div => {
                        if rhs == 0 {
                            return Err(self.error(span, ErrorKind::DivisionByZero));
                        }

                        lhs.checked_div(rhs)
                    }
                };

                value.map(LiteralKind::Int).ok_or(self.error(
                    span,
                    ErrorKind::IntegerOverflow(QualType::new(Type::Primitive(
                        crate::semantic::types::Primitive::Int,
                    ))),
                ))
            }

            _ => {
                unreachable!(
                    "Type checker should have ensured compatible types for constant folding"
                )
            }
        }
    }

    fn fold_unary(&mut self, span: Span, op: UnaryOp, value: LiteralKind) -> Result<LiteralKind> {
        match (op, value) {
            (UnaryOp::Pos, value) => Ok(value),
            (UnaryOp::Neg, LiteralKind::Int(value)) => {
                value.checked_neg().map(LiteralKind::Int).ok_or(self.error(
                    span,
                    ErrorKind::IntegerOverflow(QualType::new(Type::Primitive(
                        crate::semantic::types::Primitive::Int,
                    ))),
                ))
            }
            (UnaryOp::Neg, LiteralKind::Float(value)) => Ok(LiteralKind::Float(-value)),
            (UnaryOp::Neg, LiteralKind::Double(value)) => Ok(LiteralKind::Double(-value)),
            (UnaryOp::Neg, LiteralKind::Char(value)) => Ok(LiteralKind::Int(-(value as i32))),
        }
    }

    fn fold_cast(&self, value: LiteralKind, target_type: &QualType) -> Result<LiteralKind> {
        let target = &target_type.typ;

        if target.is_char() {
            let value = match value {
                LiteralKind::Int(value) => value as u8,
                LiteralKind::Char(value) => value as u8,
                LiteralKind::Float(value) => value as u8,
                LiteralKind::Double(value) => value as u8,
            };

            Ok(LiteralKind::Char(value))
        } else if target.is_int() {
            let value = match value {
                LiteralKind::Int(value) => value,
                LiteralKind::Char(value) => value as i32,
                LiteralKind::Float(value) => value as i32,
                LiteralKind::Double(value) => value as i32,
            };

            Ok(LiteralKind::Int(value))
        } else if target.is_float() {
            let value = match value {
                LiteralKind::Int(value) => value as f32,
                LiteralKind::Char(value) => value as f32,
                LiteralKind::Float(value) => value,
                LiteralKind::Double(value) => value as f32,
            };

            Ok(LiteralKind::Float(value))
        } else if target.is_double() {
            let value = match value {
                LiteralKind::Int(value) => value as f64,
                LiteralKind::Char(value) => value as f64,
                LiteralKind::Float(value) => value as f64,
                LiteralKind::Double(value) => value,
            };

            Ok(LiteralKind::Double(value))
        } else {
            unreachable!("Type checker should have ensured compatible types for constant folding")
        }
    }
}
