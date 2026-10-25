use std::fmt;
use std::fmt::{write, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

/// 表达式树。
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    /// `name` 或 `table.name`。
    Column { table: Option<String>, name: String },
    /// `*` 或 `table.*`。只在 SELECT 列表或 `COUNT(*)` 中合法。
    Wildcard { table: Option<String> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { left: Box<Expr>, op: BinaryOp, right: Box<Expr> },
    IsNull { expr: Box<Expr>, negated: bool },
    Between { expr: Box<Expr>, low: Box<Expr>, high: Box<Expr>, negated: bool },
    InList { expr: Box<Expr>, list: Vec<Expr>, negated: bool },
    Like { expr: Box<Expr>, pattern: Box<Expr>, negated: bool },
    Function { name: String, args: Vec<Expr>, distinct: bool },
}

impl Expr {
    pub fn column(name: &str) -> Expr {
        Expr::Column { table: None, name: name.to_string() }
    }
    pub fn int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v))
    }
    pub fn string(s: &str) -> Expr {
        Expr::Literal(Literal::String(s.to_string()))
    }
    pub fn binary(left: Expr, op: BinaryOp, right: Expr) -> Expr {
        Expr::Binary { left: Box::new(left), op, right: Box::new(right) }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Int(v) => write!(f, "{v}"),
            Literal::Float(v) => write!(f, "{v:?}"),
            Literal::String(s) => write!(f, "'{}'", s.replace('\'', "''")),
            Literal::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Literal::Null => write!(f, "NULL"),
        }
    }
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+", BinaryOp::Sub => "-", BinaryOp::Mul => "*",
            BinaryOp::Div => "/", BinaryOp::Mod => "%",
            BinaryOp::Eq => "=", BinaryOp::NotEq => "<>", BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=", BinaryOp::Gt => ">", BinaryOp::GtEq => ">=",
            BinaryOp::And => "AND", BinaryOp::Or => "OR",
        };
        f.write_str(s)
    }
}

/// 表达式打印
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let not = |negated: bool| if negated { " NOT" } else { "" };
        match self {
            Expr::Literal(lit) => write!(f, "{lit}"),
            Expr::Column { table: Some(t), name } => write!(f, "{t}.{name}"),
            Expr::Column { table: None, name } => write!(f, "{name}"),
            Expr::Wildcard { table: Some(t) } => write!(f, "{t}.*"),
            Expr::Wildcard { table: None } => write!(f, "*"),
            Expr::Unary { op: UnaryOp::Neg, expr } => write!(f, "-{expr}"),
            Expr::Unary { op: UnaryOp::Not, expr } => write!(f, "NOT {expr}"),
            Expr::Binary { left, op, right } => write!(f, "({left} {op} {right})"),
            Expr::IsNull { expr, negated } => write!(f, "{expr} IS{} NULL", not(*negated)),
            Expr::Between { expr, low, high, negated } => {
                write!(f, "{expr}{} BETWEEN {low} AND {high}", not(*negated))
            }
            Expr::InList { expr, list, negated } => {
                write!(f, "{expr}{} IN (", not(*negated))?;
                write_list(f, list)?;
                write!(f, ")")
            }
            Expr::Like { expr, pattern, negated } => write!(f, "{expr}{} LIKE {pattern}", not(*negated)),
            Expr::Function { name, args, distinct } => {
                write!(f, "{name}(")?;
                if *distinct {
                    write!(f, "DISTINCT ")?;
                }
                write_list(f, args)?;
                write!(f, ")")
            }
        }
    }
}

pub(crate) fn write_list<T: fmt::Display>(f: &mut fmt::Formatter<'_>, items: &[T]) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}