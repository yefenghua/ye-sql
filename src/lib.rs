pub mod error;
pub mod lexer;
pub mod repl;

//// 保存一行用户的输入
pub struct InputBuffer {
    buffer: String,
}

impl InputBuffer {
    pub fn new() -> Self {
        InputBuffer {
            buffer: String::new(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.buffer.trim()
    }
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}