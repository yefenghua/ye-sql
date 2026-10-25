use std::fmt;
use std::fmt::Formatter;

/// 数据库可能产生的所有错误
#[derive(Debug, Clone, PartialEq)]
pub enum DbError {
    Syntax { pos: usize, msg: String },
}

impl DbError {
    pub fn syntax(pos: usize, msg: impl Into<String>) -> Self {
        DbError::Syntax {
            pos,
            msg: msg.into(),
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Syntax { pos, msg } => write!(f, "syntax error at position {pos}: {msg}"),
        }
    }
}

impl std::error::Error for DbError {}

pub type Result<T> = std::result::Result<T, DbError>;