//! Lexer for V8 Torque using logos
//!
//! Tokenizes Torque source code into a stream of tokens.

use logos::Logos;
use std::fmt;

/// Torque tokens
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n\f]+")]  // Skip whitespace
#[logos(skip r"//[^\n]*")]      // Skip line comments
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]  // Skip block comments
pub enum Token {
    // ========== Keywords ==========
    #[token("namespace")]
    Namespace,
    #[token("type")]
    Type,
    #[token("macro")]
    Macro,
    #[token("builtin")]
    Builtin,
    #[token("javascript")]
    Javascript,
    #[token("extern")]
    Extern,
    #[token("runtime")]
    Runtime,
    #[token("const")]
    Const,
    #[token("let")]
    Let,
    #[token("class")]
    Class,
    #[token("struct")]
    Struct,
    #[token("extends")]
    Extends,
    #[token("generates")]
    Generates,
    #[token("constexpr")]
    Constexpr,
    #[token("transient")]
    Transient,
    #[token("abstract")]
    Abstract,

    // Control flow
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("return")]
    Return,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("goto")]
    Goto,
    #[token("try")]
    Try,
    #[token("label")]
    Label,
    #[token("labels")]
    Labels,
    #[token("otherwise")]
    Otherwise,
    #[token("typeswitch")]
    Typeswitch,
    #[token("case")]
    Case,
    #[token("tail")]
    Tail,
    #[token("unreachable")]
    Unreachable,

    // Assertions
    #[token("dcheck")]
    Dcheck,
    #[token("check")]
    Check,

    // Boolean literals
    #[token("true")]
    True,
    #[token("false")]
    False,

    // Operators
    #[token("operator")]
    Operator,
    #[token("implicit")]
    Implicit,
    #[token("intrinsic")]
    Intrinsic,

    // ========== Identifiers and Literals ==========
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    #[regex(r"0[xX][0-9a-fA-F]+", |lex| i64::from_str_radix(&lex.slice()[2..], 16).ok())]
    #[regex(r"[0-9]+", |lex| lex.slice().parse().ok(), priority = 2)]
    IntLiteral(i64),

    #[regex(r"[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?", |lex| lex.slice().parse().ok())]
    FloatLiteral(F64Wrapper),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())  // Strip quotes
    })]
    StringLiteral(String),

    #[regex(r"'([^'\\]|\\.)*'", |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())  // Strip quotes
    })]
    SingleStringLiteral(String),

    // ========== Punctuation ==========
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(";")]
    Semicolon,
    #[token(".")]
    Dot,
    #[token("...")]
    Ellipsis,
    #[token("@")]
    At,
    #[token("?")]
    Question,
    #[token("=>")]
    FatArrow,

    // ========== Operators ==========
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,

    #[token("==")]
    EqEq,
    #[token("!=")]
    Ne,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,

    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("!")]
    Bang,

    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token(">>>")]
    Ushr,

    #[token("=")]
    Eq,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("%=")]
    PercentEq,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,
    #[token("<<=")]
    ShlEq,
    #[token(">>=")]
    ShrEq,
    #[token(">>>=")]
    UshrEq,

    #[token("++")]
    PlusPlus,
    #[token("--")]
    MinusMinus,
}

/// Wrapper for f64 that implements Hash and Eq (using bit representation)
#[derive(Debug, Clone, Copy)]
pub struct F64Wrapper(pub f64);

impl PartialEq for F64Wrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for F64Wrapper {}

impl std::hash::Hash for F64Wrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl From<f64> for F64Wrapper {
    fn from(f: f64) -> Self {
        F64Wrapper(f)
    }
}

impl std::str::FromStr for F64Wrapper {
    type Err = std::num::ParseFloatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<f64>().map(F64Wrapper)
    }
}

impl fmt::Display for F64Wrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Ident(s) => write!(f, "{}", s),
            Token::IntLiteral(n) => write!(f, "{}", n),
            Token::FloatLiteral(n) => write!(f, "{}", n.0),
            Token::StringLiteral(s) => write!(f, "\"{}\"", s),
            Token::SingleStringLiteral(s) => write!(f, "'{}'", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// A token with its span in the source
#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub span: std::ops::Range<usize>,
}

/// Lex a source string into tokens
pub fn lex(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut lexer = Token::lexer(source);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => tokens.push(SpannedToken {
                token,
                span: lexer.span(),
            }),
            Err(()) => {
                return Err(LexError {
                    span: lexer.span(),
                    message: format!("Unexpected character: {:?}", &source[lexer.span()]),
                });
            }
        }
    }

    Ok(tokens)
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: std::ops::Range<usize>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords() {
        let tokens: Vec<_> = Token::lexer("namespace macro builtin")
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(tokens, vec![Token::Namespace, Token::Macro, Token::Builtin]);
    }

    #[test]
    fn test_ident_and_literals() {
        let tokens: Vec<_> = Token::lexer("foo 42 3.14 \"hello\"")
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(tokens, vec![
            Token::Ident("foo".to_string()),
            Token::IntLiteral(42),
            Token::FloatLiteral(F64Wrapper(3.14)),
            Token::StringLiteral("hello".to_string()),
        ]);
    }

    #[test]
    fn test_operators() {
        let tokens: Vec<_> = Token::lexer("+ - * / == != && ||")
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(tokens, vec![
            Token::Plus, Token::Minus, Token::Star, Token::Slash,
            Token::EqEq, Token::Ne, Token::AndAnd, Token::OrOr,
        ]);
    }
}
