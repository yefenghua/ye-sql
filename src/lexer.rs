use crate::error::{DbError, Result};

/// 保留字
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Select, From, Where, Insert, Into, Values, Create, Table, Drop, Index, On,
    Update, Set, Delete, And, Or, Not, Null, Is, In, Like, Between, As,
    Join, Inner, Left, Right, Outer, Cross, Group, By, Having, Order, Asc, Desc,
    Limit, Offset, Primary, Key, Unique, Default, AutoIncrement, Distinct,
    Int, Integer, Bigint, Float, Double, Varchar, Boolean, Bool, Date, Text,
    True, False, Begin, Commit, Rollback, Describe, Show, Tables, If, Exists,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword(Keyword),
    /// 表名
    Ident(String),
    /// 数值字面量
    Number(String),
    /// 字符串字面量
    String(String),
    LParen, RParen, Comma, Semicolon, Dot, Star,
    Plus, Minus, Slash, Percent,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    /// 输入结束，永远最后一个token
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// token 第一个字符在源文本中的字符偏移量。
    pub pos: usize,
}

/// 词法分析器
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Keyword {
    pub fn from_str_ci(word: &str) -> Option<Keyword> {
        use Keyword::*;
        let kw = match word.to_ascii_uppercase().as_str() {
            "SELECT" => Select, "FROM" => From, "WHERE" => Where,
            "INSERT" => Insert, "INTO" => Into, "VALUES" => Values,
            "CREATE" => Create, "TABLE" => Table, "DROP" => Drop, "INDEX" => Index, "ON" => On,
            "UPDATE" => Update, "SET" => Set, "DELETE" => Delete,
            "AND" => And, "OR" => Or, "NOT" => Not, "NULL" => Null, "IS" => Is,
            "IN" => In, "LIKE" => Like, "BETWEEN" => Between, "AS" => As,
            "JOIN" => Join, "INNER" => Inner, "LEFT" => Left, "RIGHT" => Right,
            "OUTER" => Outer, "CROSS" => Cross,
            "GROUP" => Group, "BY" => By, "HAVING" => Having, "ORDER" => Order,
            "ASC" => Asc, "DESC" => Desc, "LIMIT" => Limit, "OFFSET" => Offset,
            "PRIMARY" => Primary, "KEY" => Key, "UNIQUE" => Unique, "DEFAULT" => Default,
            "AUTO_INCREMENT" => AutoIncrement, "DISTINCT" => Distinct,
            "INT" => Int, "INTEGER" => Integer, "BIGINT" => Bigint,
            "FLOAT" => Float, "DOUBLE" => Double, "VARCHAR" => Varchar,
            "BOOLEAN" => Boolean, "BOOL" => Bool, "DATE" => Date, "TEXT" => Text,
            "TRUE" => True, "FALSE" => False,
            "BEGIN" => Begin, "COMMIT" => Commit, "ROLLBACK" => Rollback,
            "DESCRIBE" => Describe, "SHOW" => Show, "TABLES" => Tables,
            "IF" => If, "EXISTS" => Exists,
            _ => return None,
        };
        Some(kw)
    }
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    /// 跳过空格、换行以及mysql接受的注释风格
    fn skip_whitespace_and_comments(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                }
                Some('#') => self.skip_line(),
                Some('-') if self.peek_at(1) == Some('-') => self.skip_line(),
                Some('/') if self.peek_at(1) == Some('*') => {
                    let start = self.pos;
                    self.pos += 2;
                    loop {
                        match self.peek() {
                            None => return Err(DbError::syntax(start, "unterminated block comment")),
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.pos += 2;
                                break;
                            }
                            Some(_) => {
                                self.advance();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.advance() {
            if c == '\n' {
                break;
            }
        }
    }

    /// 数字处理
    fn lex_number(&mut self) -> Result<TokenKind> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.advance();
        }
        if self.peek() == Some('.') && matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) {
            self.advance();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.advance();
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            let mut look = 1;
            if matches!(self.peek_at(1), Some('+') | Some('-')) {
                look = 2;
            }
            if matches!(self.peek_at(look), Some(c) if c.is_ascii_digit()) {
                self.pos += look;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.advance();
                }
            }
        }
        if matches!(self.peek(), Some(c) if c.is_alphabetic() || c == '_') {
            return Err(DbError::syntax(start, "invalid number"));
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        Ok(TokenKind::Number(text))
    }

    /// 字符串处理
    fn lex_string(&mut self) -> Result<TokenKind> {
        let start = self.pos;
        let quote = self.advance().expect("caller checked for a quote");
        let mut out = String::new();
        loop {
            match self.advance() {
                None => return Err(DbError::syntax(start, "unterminated string literal")),
                Some('\\') => match self.advance() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('0') => out.push('\0'),
                    Some(c) => out.push(c),
                    None => return Err(DbError::syntax(start, "unterminated string literal")),
                },
                Some(c) if c == quote => {
                    if self.peek() == Some(quote) {
                        self.advance();
                        out.push(quote);
                    } else {
                        return Ok(TokenKind::String(out));
                    }
                }
                Some(c) => out.push(c),
            }
        }
    }

    /// 列名有`select`这种情况的需要单独处理
    fn lex_identifier_or_keyword(&mut self) -> Result<TokenKind> {
        let start = self.pos;
        if self.peek() == Some('`') {
            self.advance();
            let mut name = String::new();
            loop {
                match self.advance() {
                    None => return Err(DbError::syntax(start, "unterminated quoted identifier")),
                    Some('`') => return Ok(TokenKind::Ident(name)),
                    Some(c) => name.push(c),
                }
            }
        }
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.advance();
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        Ok(match Keyword::from_str_ci(&word) {
            Some(kw) => TokenKind::Keyword(kw),
            None => TokenKind::Ident(word),
        })
    }

    /// 标点符号解析
    fn lex_operator(&mut self) -> Result<TokenKind> {
        let start = self.pos;
        let c = self.advance().expect("caller checked for a character");
        let kind = match c {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '.' => TokenKind::Dot,
            '*' => TokenKind::Star,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '=' => TokenKind::Eq,
            // 可能会有<=, <>之类的情况，要额外多判断一个
            '<' => match self.peek() {
                Some('=') => { self.advance(); TokenKind::LtEq }
                Some('>') => { self.advance(); TokenKind::NotEq }
                _ => TokenKind::Lt,
            },
            '>' => match self.peek() {
                Some('=') => { self.advance(); TokenKind::GtEq }
                _ => TokenKind::Gt,
            },
            '!' => match self.peek() {
                Some('=') => { self.advance(); TokenKind::NotEq }
                _ => return Err(DbError::syntax(start, "unexpected character '!'")),
            },
            other => return Err(DbError::syntax(start, format!("unexpected character '{other}'"))),
        };
        Ok(kind)
    }

    /// 产生下一个token
    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace_and_comments()?;
        let pos = self.pos;
        let kind = match self.peek() {
            None => TokenKind::Eof,
            Some(c) if c.is_ascii_digit() => self.lex_number()?,
            Some('.') if matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) => self.lex_number()?,
            Some('\'') | Some('"') => self.lex_string()?,
            Some(c) if c.is_alphabetic() || c == '_' || c == '`' => self.lex_identifier_or_keyword()?,
            Some(_) => self.lex_operator()?,
        };
        Ok(Token { kind, pos })
    }
}

/// 对整个字符串做词法分析
pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            return Ok(tokens);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_a_select() {
        use TokenKind::*;
        assert_eq!(
            kinds("SELECT id, name FROM users WHERE age >= 30;"),
            vec![
                Keyword(super::Keyword::Select),
                Ident("id".into()), Comma, Ident("name".into()),
                Keyword(super::Keyword::From), Ident("users".into()),
                Keyword(super::Keyword::Where), Ident("age".into()),
                GtEq, Number("30".into()), Semicolon, Eof,
            ]
        );
    }

    #[test]
    fn keywords_are_case_insensitive_and_backticks_are_not() {
        assert_eq!(kinds("select")[0], TokenKind::Keyword(Keyword::Select));
        assert_eq!(kinds("SeLeCt")[0], TokenKind::Keyword(Keyword::Select));
        assert_eq!(kinds("`select`")[0], TokenKind::Ident("select".into()));
    }

    #[test]
    fn numbers() {
        assert_eq!(kinds("42")[0], TokenKind::Number("42".into()));
        assert_eq!(kinds("3.14")[0], TokenKind::Number("3.14".into()));
        assert_eq!(kinds(".5")[0], TokenKind::Number(".5".into()));
        assert_eq!(kinds("1e10")[0], TokenKind::Number("1e10".into()));
        assert_eq!(kinds("1.5e-3")[0], TokenKind::Number("1.5e-3".into()));
        // `1.` 后面跟标识符时是一个数字加一个点（为后面的 `t.col` 做准备）
        assert_eq!(kinds("1.x")[0], TokenKind::Number("1".into()));
        assert!(tokenize("12abc").is_err());
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(kinds("'hello'")[0], TokenKind::String("hello".into()));
        assert_eq!(kinds("\"hi\"")[0], TokenKind::String("hi".into()));
        assert_eq!(kinds("'it''s'")[0], TokenKind::String("it's".into()));
        assert_eq!(kinds("'a\\nb'")[0], TokenKind::String("a\nb".into()));
        assert_eq!(kinds("'don\\'t'")[0], TokenKind::String("don't".into()));
        assert!(tokenize("'open").is_err());
    }

    #[test]
    fn operators() {
        use TokenKind::*;
        assert_eq!(kinds("< <= > >= = <> != + - * / %"),
                   vec![Lt, LtEq, Gt, GtEq, Eq, NotEq, NotEq, Plus, Minus, Star, Slash, Percent, Eof]);
        assert!(tokenize("a ! b").is_err());
        assert!(tokenize("a ^ b").is_err());
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(kinds("1 -- comment\n + /* block */ 2 # tail"),
                   vec![TokenKind::Number("1".into()), TokenKind::Plus, TokenKind::Number("2".into()), TokenKind::Eof]);
        assert!(tokenize("/* never closed").is_err());
    }

    #[test]
    fn positions_point_at_token_start() {
        let toks = tokenize("ab  cd").unwrap();
        assert_eq!(toks[0].pos, 0);
        assert_eq!(toks[1].pos, 4);
        assert_eq!(toks[2].pos, 6);
    }
}