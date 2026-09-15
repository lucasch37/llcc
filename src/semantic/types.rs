#[derive(Clone, PartialEq, Debug)]
pub struct QualType {
    pub typ: Type,
    pub qualifiers: Qualifiers,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Qualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
}

impl Qualifiers {
    fn default() -> Self {
        Self {
            is_const: false,
            is_volatile: false,
            is_restrict: false,
        }
    }
}

impl QualType {
    pub fn new(typ: Type) -> QualType {
        Self {
            typ,
            qualifiers: Qualifiers::default(),
        }
    }

    pub fn type_compatible(&self, other: &QualType) -> bool {
        match (&self.typ, &other.typ) {
            (Type::Primitive(Primitive::Void), Type::Primitive(Primitive::Void)) => true,

            (Type::Primitive(Primitive::Void), Type::Primitive(_))
            | (Type::Primitive(_), Type::Primitive(Primitive::Void)) => false,

            (Type::Primitive(_), Type::Primitive(_)) => true,

            (Type::Function(f1), Type::Function(f2)) => {
                f1.return_type.type_compatible(&f2.return_type)
                    && f1.params.len() == f2.params.len()
                    && f1
                        .params
                        .iter()
                        .zip(f2.params.iter())
                        .all(|(p1, p2)| p1.type_compatible(p2))
                    && f1.variadic == f2.variadic
            }
            _ => false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Type {
    Invalid,
    Primitive(Primitive),
    // Array(Box<QualType>, ArraySize),
    // Pointer(Box<QualType>),
    // Struct(StructKind),
    // Union(StructKind),
    // Enum(Option<String>, Vec<(Token, i32)>),
    Function(FunctionType),
}

impl Type {
    pub fn is_void(&self) -> bool {
        matches!(self, Type::Primitive(Primitive::Void))
    }

    pub fn is_function(&self) -> bool {
        matches!(self, Type::Function(_))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct FunctionType {
    pub return_type: Box<QualType>,
    pub params: Vec<QualType>,
    pub variadic: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Primitive {
    Void,
    Int,
}

use std::fmt;

impl fmt::Display for QualType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.qualifiers.is_const {
            write!(f, "const ")?;
        }

        if self.qualifiers.is_volatile {
            write!(f, "volatile ")?;
        }

        if self.qualifiers.is_restrict {
            write!(f, "restrict ")?;
        }

        write!(f, "{}", self.typ)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Invalid => write!(f, "<invalid>"),

            Type::Primitive(primitive) => {
                write!(f, "{primitive}")
            }

            Type::Function(function) => {
                write!(f, "{}(", function.return_type)?;

                for (i, param) in function.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{param}")?;
                }

                if function.variadic {
                    if !function.params.is_empty() {
                        write!(f, ", ")?;
                    }

                    write!(f, "...")?;
                }

                write!(f, ")")
            }
        }
    }
}

impl fmt::Display for Primitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Primitive::Void => write!(f, "void"),
            Primitive::Int => write!(f, "int"),
        }
    }
}
