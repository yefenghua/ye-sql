use std::io::{self, BufRead, Write};
use crate::InputBuffer;
use crate::lexer::{tokenize, TokenKind};

/// 命令运行后发生了什么
#[derive(Debug, PartialEq, Eq)]
pub enum MetaCommandResult {
    Success,
    Exit,
    Unrecognized,
}

/// shell能识别的语句种类
#[derive(Debug, PartialEq, Eq)]
pub enum StatementKind {
    Insert,
    Select,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PrepareResult {
    Success(StatementKind),
    Unrecognized,
}

/// 元命令以点开头，由shell自己处理
pub fn is_meta_command(line: &str) -> bool {
    line.starts_with(".")
}

/// 判断语句类型
pub fn prepare_statement(line: &str) -> PrepareResult {
    let first_word = line.split_whitespace().next().unwrap_or("");
    match first_word.to_ascii_uppercase().as_str() {
        "INSERT" => PrepareResult::Success(StatementKind::Insert),
        "SELECT" => PrepareResult::Success(StatementKind::Select),
        _ => PrepareResult::Unrecognized,
    }
}

/// 执行已经准备好的语句
pub fn execute_statement(kind: &StatementKind) {
    match kind {
        StatementKind::Insert => println!("This is where we would do an insert."),
        StatementKind::Select => println!("This is where we would do a select."),
    }
}

/// 打印提示符但是不换行
pub fn print_prompt() {
    print!("ye-sql> ");
    io::stdout().flush().expect("failed to flush stdout");
}

pub fn read_input(reader: &mut impl BufRead, input: &mut InputBuffer) -> bool {
    input.buffer.clear();
    match reader.read_line(&mut input.buffer) {
        Ok(0) => false,
        Ok(_) => true,
        Err(err) => {
            eprintln!("error reading input: {err}");
            false
        }
    }
}

pub fn do_meta_command(line: &str) -> MetaCommandResult {
    // 拆出第一个单词，这样命令就可以带参数：
    // `.tokens SELECT 1` -> ("tokens", "SELECT 1")。
    let (cmd, arg) = match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim()),
        None => (line, ""),
    };
    match cmd {
        ".exit" | ".quit" => MetaCommandResult::Exit,
        ".help" => {
            println!(".exit            Exit the shell");
            println!(".help            Show this message");
            println!(".tables          List tables");
            println!(".tokens <sql>    Show the tokens the lexer produces");
            MetaCommandResult::Success
        }
        ".tables" => {
            println!("(no tables yet)");
            MetaCommandResult::Success
        }
        ".tokens" => {
            print_tokens(arg);
            MetaCommandResult::Success
        }
        _ => MetaCommandResult::Unrecognized,
    }
}

pub fn print_tokens(sql: &str) {
    match tokenize(sql) {
        Ok(tokens) => {
            for token in tokens {
                if token.kind == TokenKind::Eof {
                    break;
                }
                println!("{:>4}  {:?}", token.pos, token.kind);
            }
        }
        Err(err) => println!("{err}"),
    }
}

pub fn run() -> i32 {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut input = InputBuffer::new();

    loop {
        print_prompt();
        if !read_input(&mut reader, &mut input) {
            println!();
            return 0;
        }
        let line = input.as_str();
        if line.is_empty() {
            continue;
        }

        if is_meta_command(line) {
            match do_meta_command(line) {
                MetaCommandResult::Success => {}
                MetaCommandResult::Exit => return 0,
                MetaCommandResult::Unrecognized => {
                    println!("Unrecognized command '{line}'.");
                }
            }
            continue;
        }

        match prepare_statement(line) {
            PrepareResult::Success(kind) => {
                execute_statement(&kind);
                println!("Executed.");
            }
            PrepareResult::Unrecognized => {
                println!("Unrecognized keyword at start of '{line}'.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_input_strips_newline_and_detects_eof() {
        let mut reader = io::Cursor::new(b"  select 1 \n".to_vec());
        let mut input = InputBuffer::new();
        assert!(read_input(&mut reader, &mut input));
        assert_eq!(input.as_str(), "select 1");
        assert!(!read_input(&mut reader, &mut input));
    }

    #[test]
    fn meta_commands() {
        assert!(is_meta_command(".exit"));
        assert!(!is_meta_command("select 1"));
        assert_eq!(do_meta_command(".exit"), MetaCommandResult::Exit);
        assert_eq!(do_meta_command(".nope"), MetaCommandResult::Unrecognized);
    }

    #[test]
    fn prepare_recognizes_first_word() {
        assert_eq!(
            prepare_statement("insert into t values (1)"),
            PrepareResult::Success(StatementKind::Insert)
        );
        assert_eq!(
            prepare_statement("SELECT * FROM t"),
            PrepareResult::Success(StatementKind::Select)
        );
        assert_eq!(prepare_statement("drop table t"), PrepareResult::Unrecognized);
    }
}