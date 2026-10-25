use crate::ast::*;
use crate::error::{DbError, Result};
use crate::lexer::{tokenize, Keyword, Token, TokenKind};

// 优先级等级，最松的在前。数字越大结合得越紧。
const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_NOT: u8 = 3;
const PREC_CMP: u8 = 4;
const PREC_ADD: u8 = 5;
const PREC_MUL: u8 = 6;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize
}

impl Parser {
    pub fn new(src: &str) -> Result<Self> {
        Ok(Parser { tokens: tokenize(src)?, pos: 0})
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn peek_kind_at(&self, n: usize) -> &TokenKind {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
        }
        token
    }

    /// 构造一个指向当前token的语法错误
    fn error_here(&self, msg: impl Into<String>) -> DbError {
        DbError::syntax(self.peek().pos, msg)
    }

    /// 如果当前token恰好是kind，就消费掉它
    fn consume_if(&mut self, kind: &TokenKind) -> bool {
        if self.peek_kind() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume_keyword(&mut self, kw: Keyword) -> bool {
        self.consume_if(&TokenKind::Keyword(kw))
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<()> {
        if self.consume_if(kind) {
            Ok(())
        } else {
            Err(self.error_here(format!("expected {kind:?}, found {:?}", self.peek_kind())))
        }
    }

    fn expect_keyword(&mut self, kw: Keyword) -> Result<()> {
        self.expect(&TokenKind::Keyword(kw))
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.error_here(format!("expected identifier, found {other:?}"))),
        }
    }

    /// 解析一个完整的表达式
    pub fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_expr_prec(0)
    }

    /// 优先级爬升
    fn parse_expr_prec(&mut self, min_prec: u8) -> Result<Expr> {
        // 前缀 NOT 比比较运算结合得更松：`NOT a = b` 即 `NOT (a = b)`。
        let mut left = if min_prec <= PREC_NOT && self.consume_keyword(Keyword::Not) {
            let operand = self.parse_expr_prec(PREC_NOT)?;
            Expr::Unary { op: UnaryOp::Not, expr: Box::new(operand) }
        } else {
            self.parse_unary()?
        };

        loop {
            // `x NOT IN (...)`、`x NOT LIKE ...`、`x NOT BETWEEN ...`
            let negated = matches!(self.peek_kind(), TokenKind::Keyword(Keyword::Not))
                && matches!(
                    self.peek_kind_at(1),
                    TokenKind::Keyword(Keyword::In | Keyword::Like | Keyword::Between)
                );
            let op_kind = if negated { self.peek_kind_at(1).clone() } else { self.peek_kind().clone() };

            let Some(prec) = infix_precedence(&op_kind) else { break };
            if prec < min_prec {
                break;
            }
            if negated {
                self.advance();
            }
            self.advance();

            left = match op_kind {
                TokenKind::Keyword(Keyword::Between) => self.parse_between(left, negated)?,
                TokenKind::Keyword(Keyword::In) => self.parse_in(left, negated)?,
                TokenKind::Keyword(Keyword::Like) => self.parse_like(left, negated)?,
                TokenKind::Keyword(Keyword::Is) => self.parse_is(left)?,
                _ => {
                    let op = binary_op_for(&op_kind).expect("precedence table and op table agree");
                    // +1 使每个二元运算符都是左结合的。
                    let right = self.parse_expr_prec(prec + 1)?;
                    Expr::binary(left, op, right)
                }
            };
        }
        Ok(left)
    }

    /// 一元负号，以及无操作的一元正号。
    fn parse_unary(&mut self) -> Result<Expr> {
        if self.consume_if(&TokenKind::Minus) {
            let expr = self.parse_unary()?;
            // 立刻把 `-5` 折叠成负的字面量。
            return Ok(match expr {
                Expr::Literal(Literal::Int(v)) => Expr::Literal(Literal::Int(-v)),
                Expr::Literal(Literal::Float(v)) => Expr::Literal(Literal::Float(-v)),
                other => Expr::Unary { op: UnaryOp::Neg, expr: Box::new(other) },
            });
        }
        if self.consume_if(&TokenKind::Plus) {
            return self.parse_unary();
        }
        self.parse_primary()
    }

    /// 字面量、列引用、`*`、带括号的表达式和函数调用。
    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.advance();
        match token.kind {
            TokenKind::Number(text) => parse_number_literal(&text, token.pos),
            TokenKind::String(s) => Ok(Expr::Literal(Literal::String(s))),
            TokenKind::Keyword(Keyword::True) => Ok(Expr::Literal(Literal::Bool(true))),
            TokenKind::Keyword(Keyword::False) => Ok(Expr::Literal(Literal::Bool(false))),
            TokenKind::Keyword(Keyword::Null) => Ok(Expr::Literal(Literal::Null)),
            TokenKind::Star => Ok(Expr::Wildcard { table: None }),
            TokenKind::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(inner)
            }
            TokenKind::Ident(name) => {
                if self.peek_kind() == &TokenKind::LParen {
                    return self.parse_function_call(name);
                }
                if self.consume_if(&TokenKind::Dot) {
                    if self.consume_if(&TokenKind::Star) {
                        return Ok(Expr::Wildcard { table: Some(name) });
                    }
                    let column = self.expect_ident()?;
                    return Ok(Expr::Column { table: Some(name), name: column });
                }
                Ok(Expr::Column { table: None, name })
            }
            other => Err(DbError::syntax(token.pos, format!("unexpected {other:?} in expression"))),
        }
    }

    /// `name(args)`、`name(*)`、`name(DISTINCT arg)`。到达这里时，
    /// 左括号仍是当前 token。
    fn parse_function_call(&mut self, name: String) -> Result<Expr> {
        self.expect(&TokenKind::LParen)?;
        let distinct = self.consume_keyword(Keyword::Distinct);
        let mut args = Vec::new();
        if !self.consume_if(&TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.consume_if(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen)?;
        }
        Ok(Expr::Function { name: name.to_ascii_uppercase(), args, distinct })
    }

    /// `expr [NOT] BETWEEN low AND high`。边界以高于 AND 的优先级解析，
    /// 这样 AND 不会被误认为逻辑 AND。
    fn parse_between(&mut self, expr: Expr, negated: bool) -> Result<Expr> {
        let low = self.parse_expr_prec(PREC_ADD)?;
        self.expect_keyword(Keyword::And)?;
        let high = self.parse_expr_prec(PREC_ADD)?;
        Ok(Expr::Between { expr: Box::new(expr), low: Box::new(low), high: Box::new(high), negated })
    }

    /// `expr [NOT] IN (a, b, c)`。
    fn parse_in(&mut self, expr: Expr, negated: bool) -> Result<Expr> {
        self.expect(&TokenKind::LParen)?;
        let mut list = Vec::new();
        loop {
            list.push(self.parse_expr()?);
            if !self.consume_if(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        Ok(Expr::InList { expr: Box::new(expr), list, negated })
    }

    /// `expr [NOT] LIKE pattern`。
    fn parse_like(&mut self, expr: Expr, negated: bool) -> Result<Expr> {
        let pattern = self.parse_expr_prec(PREC_ADD)?;
        Ok(Expr::Like { expr: Box::new(expr), pattern: Box::new(pattern), negated })
    }

    /// `expr IS [NOT] NULL`。IS 关键字已经被消费掉了。
    fn parse_is(&mut self, expr: Expr) -> Result<Expr> {
        let negated = self.consume_keyword(Keyword::Not);
        self.expect_keyword(Keyword::Null)?;
        Ok(Expr::IsNull { expr: Box::new(expr), negated })
    }
}

/// 中缀运算符 token 的优先级
fn infix_precedence(kind: &TokenKind) -> Option<u8> {
    Some(match kind {
        TokenKind::Keyword(Keyword::Or) => PREC_OR,
        TokenKind::Keyword(Keyword::And) => PREC_AND,
        TokenKind::Eq | TokenKind::NotEq | TokenKind::Lt
        | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq => PREC_CMP,
        TokenKind::Keyword(Keyword::Is | Keyword::In | Keyword::Like | Keyword::Between) => PREC_CMP,
        TokenKind::Plus | TokenKind::Minus => PREC_ADD,
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => PREC_MUL,
        _ => return None,
    })
}

/// 把运算符 token 映射到它所表示的 AST 运算符。
fn binary_op_for(kind: &TokenKind) -> Option<BinaryOp> {
    Some(match kind {
        TokenKind::Plus => BinaryOp::Add,
        TokenKind::Minus => BinaryOp::Sub,
        TokenKind::Star => BinaryOp::Mul,
        TokenKind::Slash => BinaryOp::Div,
        TokenKind::Percent => BinaryOp::Mod,
        TokenKind::Eq => BinaryOp::Eq,
        TokenKind::NotEq => BinaryOp::NotEq,
        TokenKind::Lt => BinaryOp::Lt,
        TokenKind::LtEq => BinaryOp::LtEq,
        TokenKind::Gt => BinaryOp::Gt,
        TokenKind::GtEq => BinaryOp::GtEq,
        TokenKind::Keyword(Keyword::And) => BinaryOp::And,
        TokenKind::Keyword(Keyword::Or) => BinaryOp::Or,
        _ => return None,
    })
}

/// 把 lexer 给出的数字文本转成整数（装得下时），否则转成浮点数。
fn parse_number_literal(text: &str, pos: usize) -> Result<Expr> {
    if let Ok(v) = text.parse::<i64>() {
        return Ok(Expr::Literal(Literal::Int(v)));
    }
    text.parse::<f64>()
        .map(|v| Expr::Literal(Literal::Float(v)))
        .map_err(|_| DbError::syntax(pos, format!("invalid number '{text}'")))
}

/// 解析一个应当恰好包含一个表达式的字符串。
pub fn parse_expression(src: &str) -> Result<Expr> {
    let mut parser = Parser::new(src)?;
    let expr = parser.parse_expr()?;
    if parser.peek_kind() != &TokenKind::Eof {
        return Err(parser.error_here(format!("unexpected {:?} after expression", parser.peek_kind())));
    }
    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> String {
        parse_expression(src).unwrap().to_string()
    }

    #[test]
    fn arithmetic_precedence_and_associativity() {
        assert_eq!(p("a + b * 2"), "(a + (b * 2))");
        assert_eq!(p("a - b - c"), "((a - b) - c)");
        assert_eq!(p("(a + b) * c"), "((a + b) * c)");
        assert_eq!(p("a / b % c"), "((a / b) % c)");
    }

    #[test]
    fn logical_precedence() {
        assert_eq!(p("a OR b AND c"), "(a OR (b AND c))");
        assert_eq!(p("NOT a = 1"), "NOT (a = 1)");
        assert_eq!(p("NOT a AND b"), "(NOT a AND b)");
        assert_eq!(p("a + b * 2 > 10 AND NOT (c IS NULL)"),
                   "(((a + (b * 2)) > 10) AND NOT c IS NULL)");
    }

    #[test]
    fn unary_minus_folds_literals() {
        assert_eq!(parse_expression("-5").unwrap(), Expr::int(-5));
        assert_eq!(p("-a"), "-a");
        assert_eq!(p("1 - -2"), "(1 - -2)");
    }

    #[test]
    fn literals() {
        assert_eq!(parse_expression("3.5").unwrap(), Expr::Literal(Literal::Float(3.5)));
        assert_eq!(parse_expression("'x'").unwrap(), Expr::string("x"));
        assert_eq!(parse_expression("TRUE").unwrap(), Expr::Literal(Literal::Bool(true)));
        assert_eq!(parse_expression("null").unwrap(), Expr::Literal(Literal::Null));
        assert_eq!(p("'it''s'"), "'it''s'");
    }

    #[test]
    fn columns_and_wildcards() {
        assert_eq!(parse_expression("t.name").unwrap(),
                   Expr::Column { table: Some("t".into()), name: "name".into() });
        assert_eq!(p("*"), "*");
        assert_eq!(p("t.*"), "t.*");
    }

    #[test]
    fn postfix_predicates() {
        assert_eq!(p("x IS NULL"), "x IS NULL");
        assert_eq!(p("x IS NOT NULL"), "x IS NOT NULL");
        assert_eq!(p("x BETWEEN 1 AND 10 AND y"), "(x BETWEEN 1 AND 10 AND y)");
        assert_eq!(p("x NOT BETWEEN 1 + 1 AND 10"), "x NOT BETWEEN (1 + 1) AND 10");
        assert_eq!(p("x IN (1, 2, 3)"), "x IN (1, 2, 3)");
        assert_eq!(p("x NOT IN ('a')"), "x NOT IN ('a')");
        assert_eq!(p("name LIKE 'A%'"), "name LIKE 'A%'");
        assert_eq!(p("name NOT LIKE 'A%' OR 1"), "(name NOT LIKE 'A%' OR 1)");
    }

    #[test]
    fn function_calls() {
        assert_eq!(p("count(*)"), "COUNT(*)");
        assert_eq!(p("COUNT(DISTINCT x)"), "COUNT(DISTINCT x)");
        assert_eq!(p("upper(name)"), "UPPER(name)");
        assert_eq!(p("now()"), "NOW()");
        assert_eq!(p("coalesce(a, b, 0) + 1"), "(COALESCE(a, b, 0) + 1)");
    }

    #[test]
    fn errors_point_at_the_problem() {
        let err = parse_expression("a + ").unwrap_err();
        assert!(matches!(err, DbError::Syntax { pos: 4, .. }), "{err:?}");
        let err = parse_expression("a b").unwrap_err();
        assert!(matches!(err, DbError::Syntax { pos: 2, .. }), "{err:?}");
        assert!(parse_expression("(a").is_err());
        assert!(parse_expression("x BETWEEN 1").is_err());
    }
}