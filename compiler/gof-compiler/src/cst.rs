use crate::token::Token;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CstModule {
    pub tokens: Vec<Token>,
}

impl CstModule {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens }
    }
}
