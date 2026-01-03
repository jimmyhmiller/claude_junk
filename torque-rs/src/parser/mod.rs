//! Parser for V8 Torque using chumsky
//!
//! Parses a token stream into an AST with full Torque language support.

use chumsky::prelude::*;
use crate::ast::*;
use crate::lexer::Token;

type ParserInput = Vec<(Token, Span)>;
type ParserError = Simple<Token>;

/// Main entry point: parse a source file
pub fn parse(tokens: ParserInput) -> Result<SourceFile, Vec<ParserError>> {
    let len = tokens.len();
    let stream = chumsky::Stream::from_iter(len..len + 1, tokens.into_iter());
    file_parser().parse(stream)
}

// ============================================================================
// Helpers
// ============================================================================

fn ident() -> impl Parser<Token, Ident, Error = ParserError> + Clone {
    filter_map(|span, tok| match tok {
        Token::Ident(s) => Ok(Ident::new(s, span)),
        _ => Err(Simple::expected_input_found(span, Vec::new(), Some(tok))),
    })
}

fn string_literal() -> impl Parser<Token, String, Error = ParserError> + Clone {
    filter_map(|span, tok| match tok {
        Token::StringLiteral(s) | Token::SingleStringLiteral(s) => Ok(s),
        _ => Err(Simple::expected_input_found(span, Vec::new(), Some(tok))),
    })
}

// ============================================================================
// Annotations: @export, @noVerifier, @if(...), etc.
// ============================================================================

fn annotation() -> impl Parser<Token, Annotation, Error = ParserError> + Clone {
    just(Token::At)
        .ignore_then(ident())
        .then(
            // Optional arguments in parentheses
            filter_map(|span, tok| match tok {
                Token::Ident(s) => Ok(s),
                Token::StringLiteral(s) => Ok(s),
                Token::IntLiteral(n) => Ok(n.to_string()),
                _ => Err(Simple::expected_input_found(span, Vec::new(), Some(tok))),
            })
            .separated_by(just(Token::Comma))
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .or_not()
            .map(|a| a.unwrap_or_default()),
        )
        .map(|(name, args)| Annotation { name, args })
}

fn annotations() -> impl Parser<Token, Vec<Annotation>, Error = ParserError> + Clone {
    annotation().repeated()
}

// ============================================================================
// Generic Type Parameters: <T, U extends Foo>
// ============================================================================

fn type_params() -> impl Parser<Token, Vec<Ident>, Error = ParserError> + Clone {
    ident()
        .separated_by(just(Token::Comma))
        .delimited_by(just(Token::Lt), just(Token::Gt))
        .or_not()
        .map(|p| p.unwrap_or_default())
}

// ============================================================================
// Top-Level
// ============================================================================

fn file_parser() -> impl Parser<Token, SourceFile, Error = ParserError> {
    declaration()
        .map_with_span(Spanned::new)
        .repeated()
        .then_ignore(end())
        .map(|declarations| SourceFile { declarations })
}

fn declaration() -> impl Parser<Token, Declaration, Error = ParserError> + Clone {
    recursive(|decl| {
        let namespace = namespace_decl(decl.clone());
        let type_decl = type_declaration();
        let builtin = builtin_decl();
        let macro_decl = macro_declaration();
        let const_decl = const_declaration();
        let extern_decl = extern_declaration();
        let class_decl = class_declaration(decl.clone());
        let struct_decl = struct_declaration();

        choice((
            namespace.map(Declaration::Namespace),
            type_decl.map(Declaration::Type),
            extern_decl.map(Declaration::Extern),
            builtin.map(Declaration::Builtin),
            macro_decl.map(Declaration::Macro),
            const_decl.map(Declaration::Const),
            class_decl.map(Declaration::Class),
            struct_decl.map(Declaration::Struct),
        ))
    })
}

// ============================================================================
// Declarations
// ============================================================================

fn namespace_decl(
    decl: impl Parser<Token, Declaration, Error = ParserError> + Clone,
) -> impl Parser<Token, NamespaceDecl, Error = ParserError> + Clone {
    just(Token::Namespace)
        .ignore_then(ident())
        .then(
            decl.map_with_span(Spanned::new)
                .repeated()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(name, declarations)| NamespaceDecl { name, declarations })
}

fn type_declaration() -> impl Parser<Token, TypeDecl, Error = ParserError> + Clone {
    let transient = just(Token::Transient).or_not().map(|t| t.is_some());

    let extends = just(Token::Extends).ignore_then(type_expr());
    let generates = just(Token::Generates).ignore_then(string_literal());
    let constexpr = just(Token::Constexpr).ignore_then(string_literal());

    transient
        .then_ignore(just(Token::Type))
        .then(ident())
        .then(type_params())
        .then(extends.or_not())
        .then(generates.or_not())
        .then(constexpr.or_not())
        .then_ignore(just(Token::Semicolon))
        .map(
            |(((((is_transient, name), type_params), extends), generates), constexpr)| TypeDecl {
                name,
                type_params,
                extends,
                generates,
                constexpr,
                is_transient,
            },
        )
}

fn builtin_decl() -> impl Parser<Token, BuiltinDecl, Error = ParserError> + Clone {
    let js = just(Token::Javascript).or_not().map(|j| j.is_some());

    annotations()
        .then(js)
        .then_ignore(just(Token::Builtin))
        .then(ident())
        .then(type_params())
        .then(parameters())
        .then(return_type().or_not())
        .then(block().or_not())
        .map(
            |((((((annots, is_javascript), name), type_params), params), return_type), body)| {
                BuiltinDecl {
                    annotations: annots,
                    is_javascript,
                    name,
                    type_params,
                    params,
                    return_type,
                    body,
                }
            },
        )
}

fn macro_declaration() -> impl Parser<Token, MacroDecl, Error = ParserError> + Clone {
    annotations()
        .then_ignore(just(Token::Macro))
        .then(ident())
        .then(type_params())
        .then(parameters())
        .then(return_type().or_not())
        .then(labels_clause().or_not().map(|l| l.unwrap_or_default()))
        .then(block().or_not())
        .map(
            |((((((annots, name), type_params), params), return_type), labels), body)| MacroDecl {
                annotations: annots,
                name,
                type_params,
                params,
                return_type,
                labels,
                body,
            },
        )
}

fn const_declaration() -> impl Parser<Token, ConstDecl, Error = ParserError> + Clone {
    just(Token::Const)
        .ignore_then(ident())
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .then_ignore(just(Token::Eq))
        .then(expr_parser().map_with_span(Spanned::new))
        .then_ignore(just(Token::Semicolon))
        .map(|((name, type_expr), value)| ConstDecl {
            name,
            type_expr,
            value,
        })
}

// ============================================================================
// Extern Declarations: extern macro, extern builtin, extern runtime
// ============================================================================

fn extern_declaration() -> impl Parser<Token, ExternDecl, Error = ParserError> + Clone {
    let extern_macro = annotations()
        .then_ignore(just(Token::Extern))
        .then_ignore(just(Token::Macro))
        .then(ident())
        .then(type_params())
        .then(parameters())
        .then(return_type().or_not())
        .then(labels_clause().or_not().map(|l| l.unwrap_or_default()))
        .then_ignore(just(Token::Semicolon))
        .map(
            |(((((annots, name), type_params), params), return_type), labels)| MacroDecl {
                annotations: annots,
                name,
                type_params,
                params,
                return_type,
                labels,
                body: None,
            },
        )
        .map(ExternDecl::Macro);

    let extern_builtin = annotations()
        .then_ignore(just(Token::Extern))
        .then(just(Token::Javascript).or_not().map(|j| j.is_some()))
        .then_ignore(just(Token::Builtin))
        .then(ident())
        .then(type_params())
        .then(parameters())
        .then(return_type().or_not())
        .then_ignore(just(Token::Semicolon))
        .map(
            |(((((annots, is_javascript), name), type_params), params), return_type)| BuiltinDecl {
                annotations: annots,
                is_javascript,
                name,
                type_params,
                params,
                return_type,
                body: None,
            },
        )
        .map(ExternDecl::Builtin);

    let extern_runtime = just(Token::Extern)
        .ignore_then(just(Token::Runtime))
        .ignore_then(ident())
        .then(parameters())
        .then(return_type().or_not())
        .then_ignore(just(Token::Semicolon))
        .map(|((name, params), return_type)| RuntimeDecl {
            name,
            params,
            return_type,
        })
        .map(ExternDecl::Runtime);

    choice((extern_macro, extern_builtin, extern_runtime))
}

// ============================================================================
// Class and Struct Declarations
// ============================================================================

fn class_declaration(
    decl: impl Parser<Token, Declaration, Error = ParserError> + Clone,
) -> impl Parser<Token, ClassDecl, Error = ParserError> + Clone {
    let is_extern = just(Token::Extern).or_not().map(|e| e.is_some());

    annotations()
        .then(is_extern)
        .then_ignore(just(Token::Class))
        .then(ident())
        .then(type_params())
        .then(just(Token::Extends).ignore_then(type_expr()).or_not())
        .then(
            class_body(decl)
                .or_not()
                .map(|opt| opt.unwrap_or_else(|| (vec![], vec![]))),
        )
        .map(
            |(((((annots, is_extern), name), type_params), extends), (fields, methods))| ClassDecl {
                annotations: annots,
                is_extern,
                name,
                type_params,
                extends,
                fields,
                methods,
            },
        )
}

fn class_body(
    decl: impl Parser<Token, Declaration, Error = ParserError> + Clone,
) -> impl Parser<Token, (Vec<ClassField>, Vec<Spanned<Declaration>>), Error = ParserError> + Clone {
    let field = annotations()
        .then(just(Token::Ident("weak".to_string())).or_not().map(|w| w.is_some()))
        .then(ident())
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .then_ignore(just(Token::Semicolon))
        .map(|(((annots, is_weak), name), type_expr)| ClassField {
            annotations: annots,
            name,
            type_expr,
            is_weak,
        });

    let method = decl.map_with_span(Spanned::new);

    choice((
        field.map(ClassBodyItem::Field),
        method.map(ClassBodyItem::Method),
    ))
    .repeated()
    .delimited_by(just(Token::LBrace), just(Token::RBrace))
    .map(|items| {
        let mut fields = vec![];
        let mut methods = vec![];
        for item in items {
            match item {
                ClassBodyItem::Field(f) => fields.push(f),
                ClassBodyItem::Method(m) => methods.push(m),
            }
        }
        (fields, methods)
    })
}

enum ClassBodyItem {
    Field(ClassField),
    Method(Spanned<Declaration>),
}

fn struct_declaration() -> impl Parser<Token, StructDecl, Error = ParserError> + Clone {
    just(Token::Struct)
        .ignore_then(ident())
        .then(type_params())
        .then(
            struct_field()
                .repeated()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((name, type_params), fields)| StructDecl {
            name,
            type_params,
            fields,
        })
}

fn struct_field() -> impl Parser<Token, StructField, Error = ParserError> + Clone {
    ident()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .then_ignore(just(Token::Semicolon))
        .map(|(name, type_expr)| StructField { name, type_expr })
}

// ============================================================================
// Parameters and Types
// ============================================================================

fn parameters() -> impl Parser<Token, Vec<Parameter>, Error = ParserError> + Clone {
    // Handle both implicit and regular parameters
    let implicit_param = just(Token::Implicit)
        .ignore_then(ident())
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, type_expr)| Parameter {
            name,
            type_expr,
            is_implicit: true,
            is_rest: false,
        });

    let regular_param = {
        let rest = just(Token::Ellipsis).or_not().map(|r| r.is_some());
        rest.then(ident())
            .then_ignore(just(Token::Colon))
            .then(type_expr())
            .map(|((is_rest, name), type_expr)| Parameter {
                name,
                type_expr,
                is_implicit: false,
                is_rest,
            })
    };

    choice((implicit_param, regular_param))
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .delimited_by(just(Token::LParen), just(Token::RParen))
}

fn return_type() -> impl Parser<Token, TypeExpr, Error = ParserError> + Clone {
    just(Token::Colon).ignore_then(type_expr())
}

fn labels_clause() -> impl Parser<Token, Vec<LabelDecl>, Error = ParserError> + Clone {
    just(Token::Labels).ignore_then(label_decl().separated_by(just(Token::Comma)).at_least(1))
}

fn label_decl() -> impl Parser<Token, LabelDecl, Error = ParserError> + Clone {
    ident()
        .then(
            type_expr()
                .separated_by(just(Token::Comma))
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not()
                .map(|p| p.unwrap_or_default()),
        )
        .map(|(name, params)| LabelDecl { name, params })
}

fn type_expr() -> impl Parser<Token, TypeExpr, Error = ParserError> + Clone {
    recursive(|ty| {
        let named = ident().map(TypeExpr::Named);

        let generic = ident()
            .then(
                ty.clone()
                    .separated_by(just(Token::Comma))
                    .delimited_by(just(Token::Lt), just(Token::Gt)),
            )
            .map(|(name, args)| TypeExpr::Generic { name, args });

        // Function type: (A, B) => C
        let func_type = ty
            .clone()
            .separated_by(just(Token::Comma))
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .then_ignore(just(Token::FatArrow))
            .then(ty.clone())
            .map(|(params, ret)| TypeExpr::Function {
                params,
                return_type: Box::new(ret),
            });

        let base = choice((func_type, generic, named));

        // Union types: A | B
        base.clone()
            .then(just(Token::Pipe).ignore_then(base).repeated())
            .map(|(first, rest)| {
                if rest.is_empty() {
                    first
                } else {
                    let mut types = vec![first];
                    types.extend(rest);
                    TypeExpr::Union(types)
                }
            })
    })
}

// ============================================================================
// Statements - Using boxed recursive parsers
// ============================================================================

fn block() -> impl Parser<Token, Block, Error = ParserError> + Clone {
    recursive(|block_ref| {
        statement_inner(block_ref)
            .map_with_span(Spanned::new)
            .repeated()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(|statements| Block { statements })
    })
}

fn statement_inner(
    block_ref: impl Parser<Token, Block, Error = ParserError> + Clone + 'static,
) -> impl Parser<Token, Statement, Error = ParserError> + Clone {
    let var_decl = {
        let kw = choice((just(Token::Const).to(true), just(Token::Let).to(false)));

        kw.then(ident())
            .then(just(Token::Colon).ignore_then(type_expr()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr_parser().map_with_span(Spanned::new))
            .then_ignore(just(Token::Semicolon))
            .map(|(((is_const, name), type_expr), init)| Statement::VarDecl {
                is_const,
                name,
                type_expr,
                init,
            })
    };

    let return_stmt = just(Token::Return)
        .ignore_then(expr_parser().map_with_span(Spanned::new).or_not())
        .then_ignore(just(Token::Semicolon))
        .map(Statement::Return);

    let block_ref_clone = block_ref.clone();
    let block_ref_clone2 = block_ref.clone();
    let block_ref_clone3 = block_ref.clone();
    let block_ref_clone4 = block_ref.clone();

    let if_stmt = just(Token::If)
        .ignore_then(expr_parser().delimited_by(just(Token::LParen), just(Token::RParen)))
        .then(block_ref.clone())
        .then(just(Token::Else).ignore_then(block_ref.clone()).or_not())
        .map(move |((cond, then_branch), else_branch)| Statement::If {
            condition: Spanned::new(cond, 0..0),
            then_branch,
            else_branch,
        });

    let while_stmt = just(Token::While)
        .ignore_then(expr_parser().delimited_by(just(Token::LParen), just(Token::RParen)))
        .then(block_ref_clone.clone())
        .map(|(cond, body)| Statement::While {
            condition: Spanned::new(cond, 0..0),
            body,
        });

    // For loop: for (init; cond; update) { ... }
    let for_init = choice((
        {
            let kw = choice((just(Token::Const).to(true), just(Token::Let).to(false)));
            kw.then(ident())
                .then(just(Token::Colon).ignore_then(type_expr()).or_not())
                .then_ignore(just(Token::Eq))
                .then(expr_parser().map_with_span(Spanned::new))
                .map(|(((is_const, name), type_expr), init)| Statement::VarDecl {
                    is_const,
                    name,
                    type_expr,
                    init,
                })
                .map_with_span(|s, span| Some(Box::new(Spanned::new(s, span))))
        },
        expr_parser()
            .map_with_span(|e, span| Statement::Expr(Spanned::new(e, span)))
            .map_with_span(|s, span| Some(Box::new(Spanned::new(s, span)))),
        just(Token::Semicolon).to(()).rewind().to(None),
    ));

    let for_stmt = just(Token::For)
        .ignore_then(just(Token::LParen))
        .ignore_then(for_init)
        .then_ignore(just(Token::Semicolon))
        .then(expr_parser().map_with_span(Spanned::new).or_not())
        .then_ignore(just(Token::Semicolon))
        .then(expr_parser().map_with_span(Spanned::new).or_not())
        .then_ignore(just(Token::RParen))
        .then(block_ref_clone2.clone())
        .map(|(((init, condition), update), body)| Statement::For {
            init,
            condition,
            update,
            body,
        });

    let goto_stmt = just(Token::Goto)
        .ignore_then(ident())
        .then(
            expr_parser()
                .map_with_span(Spanned::new)
                .separated_by(just(Token::Comma))
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not()
                .map(|a| a.unwrap_or_default()),
        )
        .then_ignore(just(Token::Semicolon))
        .map(|(label, args)| Statement::Goto { label, args });

    let break_stmt = just(Token::Break)
        .then_ignore(just(Token::Semicolon))
        .to(Statement::Break);

    let continue_stmt = just(Token::Continue)
        .then_ignore(just(Token::Semicolon))
        .to(Statement::Continue);

    let unreachable_stmt = just(Token::Unreachable)
        .then_ignore(just(Token::Semicolon))
        .to(Statement::Unreachable);

    let typeswitch_stmt = just(Token::Typeswitch)
        .ignore_then(expr_parser().delimited_by(just(Token::LParen), just(Token::RParen)))
        .then(
            typeswitch_case(block_ref_clone3.clone())
                .repeated()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(value, cases)| Statement::Typeswitch {
            value: Spanned::new(value, 0..0),
            cases,
        });

    // Try with label blocks: try { ... } label Fail { ... }
    let try_stmt = just(Token::Try)
        .ignore_then(block_ref_clone4.clone())
        .then(label_block(block_ref_clone4).repeated())
        .map(|(body, handlers)| Statement::Try { body, handlers });

    // Assertions: dcheck(...); check(...);
    let dcheck_stmt = just(Token::Dcheck)
        .ignore_then(expr_parser().delimited_by(just(Token::LParen), just(Token::RParen)))
        .then_ignore(just(Token::Semicolon))
        .map(|e| Statement::Assert {
            kind: AssertKind::Dcheck,
            condition: Spanned::new(e, 0..0),
        });

    let check_stmt = just(Token::Check)
        .ignore_then(expr_parser().delimited_by(just(Token::LParen), just(Token::RParen)))
        .then_ignore(just(Token::Semicolon))
        .map(|e| Statement::Assert {
            kind: AssertKind::Check,
            condition: Spanned::new(e, 0..0),
        });

    let expr_stmt = expr_parser()
        .map_with_span(Spanned::new)
        .then_ignore(just(Token::Semicolon))
        .map(Statement::Expr);

    choice((
        var_decl,
        return_stmt,
        if_stmt,
        while_stmt,
        for_stmt,
        goto_stmt,
        break_stmt,
        continue_stmt,
        unreachable_stmt,
        typeswitch_stmt,
        try_stmt,
        dcheck_stmt,
        check_stmt,
        expr_stmt,
    ))
}

fn label_block(
    block_ref: impl Parser<Token, Block, Error = ParserError> + Clone,
) -> impl Parser<Token, LabelBlock, Error = ParserError> + Clone {
    just(Token::Label)
        .ignore_then(ident())
        .then(
            parameter()
                .separated_by(just(Token::Comma))
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not()
                .map(|p| p.unwrap_or_default()),
        )
        .then(block_ref)
        .map(|((name, params), body)| LabelBlock { name, params, body })
}

fn parameter() -> impl Parser<Token, Parameter, Error = ParserError> + Clone {
    let rest = just(Token::Ellipsis).or_not().map(|r| r.is_some());

    rest.then(ident())
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|((is_rest, name), type_expr)| Parameter {
            name,
            type_expr,
            is_implicit: false,
            is_rest,
        })
}

fn typeswitch_case(
    block_ref: impl Parser<Token, Block, Error = ParserError> + Clone,
) -> impl Parser<Token, TypeswitchCase, Error = ParserError> + Clone {
    just(Token::Case)
        .ignore_then(
            ident()
                .then_ignore(just(Token::Colon))
                .then(type_expr())
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then_ignore(just(Token::Colon))
        .then(block_ref)
        .map(|((binding, type_expr), body)| TypeswitchCase {
            binding,
            type_expr,
            body,
        })
}

// ============================================================================
// Expression Parser - Iterative Pratt-style to avoid stack overflow
// ============================================================================

/// Operator precedence levels (higher = binds tighter)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Assign = 0,   // = (right-associative, lowest precedence)
    Or = 1,       // ||
    And = 2,      // &&
    BitOr = 3,    // |
    BitXor = 4,   // ^
    BitAnd = 5,   // &
    Equality = 6, // == !=
    Compare = 7,  // < > <= >=
    Shift = 8,    // << >> >>>
    Term = 9,     // + -
    Factor = 10,  // * / %
}

fn get_binary_precedence(tok: &Token) -> Option<(Precedence, BinaryOp)> {
    match tok {
        Token::Eq => Some((Precedence::Assign, BinaryOp::Assign)),
        Token::OrOr => Some((Precedence::Or, BinaryOp::Or)),
        Token::AndAnd => Some((Precedence::And, BinaryOp::And)),
        Token::Pipe => Some((Precedence::BitOr, BinaryOp::BitOr)),
        Token::Caret => Some((Precedence::BitXor, BinaryOp::BitXor)),
        Token::Amp => Some((Precedence::BitAnd, BinaryOp::BitAnd)),
        Token::EqEq => Some((Precedence::Equality, BinaryOp::Eq)),
        Token::Ne => Some((Precedence::Equality, BinaryOp::Ne)),
        Token::Lt => Some((Precedence::Compare, BinaryOp::Lt)),
        Token::Le => Some((Precedence::Compare, BinaryOp::Le)),
        Token::Gt => Some((Precedence::Compare, BinaryOp::Gt)),
        Token::Ge => Some((Precedence::Compare, BinaryOp::Ge)),
        Token::Shl => Some((Precedence::Shift, BinaryOp::Shl)),
        Token::Shr => Some((Precedence::Shift, BinaryOp::Shr)),
        Token::Ushr => Some((Precedence::Shift, BinaryOp::Ushr)),
        Token::Plus => Some((Precedence::Term, BinaryOp::Add)),
        Token::Minus => Some((Precedence::Term, BinaryOp::Sub)),
        Token::Star => Some((Precedence::Factor, BinaryOp::Mul)),
        Token::Slash => Some((Precedence::Factor, BinaryOp::Div)),
        Token::Percent => Some((Precedence::Factor, BinaryOp::Mod)),
        _ => None,
    }
}

/// Non-recursive expression parser using Pratt parsing
fn expr_parser() -> impl Parser<Token, Expr, Error = ParserError> + Clone {
    recursive(|expr| {
        // Atoms (non-recursive base cases)
        let int_lit = filter_map(|span, tok| match tok {
            Token::IntLiteral(n) => Ok(Expr::IntLiteral(n)),
            _ => Err(Simple::expected_input_found(span, Vec::new(), Some(tok))),
        });

        let float_lit = filter_map(|span, tok| match tok {
            Token::FloatLiteral(n) => Ok(Expr::FloatLiteral(n)),
            _ => Err(Simple::expected_input_found(span, Vec::new(), Some(tok))),
        });

        let string_lit = filter_map(|span, tok| match tok {
            Token::StringLiteral(s) | Token::SingleStringLiteral(s) => Ok(Expr::StringLiteral(s)),
            _ => Err(Simple::expected_input_found(span, Vec::new(), Some(tok))),
        });

        let bool_lit = choice((
            just(Token::True).to(Expr::BoolLiteral(true)),
            just(Token::False).to(Expr::BoolLiteral(false)),
        ));

        let ident_expr = ident().map(Expr::Ident);

        let paren = expr
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map(|e| Expr::Paren(Box::new(Spanned::new(e, 0..0))));

        let atom = choice((int_lit, float_lit, string_lit, bool_lit, paren, ident_expr));

        // Unary prefix operators
        let unary_op = choice((
            just(Token::Bang).to(UnaryOp::Not),
            just(Token::Minus).to(UnaryOp::Neg),
            just(Token::Tilde).to(UnaryOp::BitNot),
        ));

        let unary = unary_op
            .repeated()
            .then(atom)
            .foldr(|op, e| Expr::Unary {
                op,
                operand: Box::new(Spanned::new(e, 0..0)),
            });

        // Generic type arguments for calls: Foo<T>(...)
        let type_args = type_expr()
            .separated_by(just(Token::Comma))
            .delimited_by(just(Token::Lt), just(Token::Gt))
            .or_not()
            .map(|a| a.unwrap_or_default());

        // Otherwise clause for calls: Call() otherwise Label
        let otherwise_clause = just(Token::Otherwise)
            .ignore_then(ident().separated_by(just(Token::Comma)).at_least(1))
            .or_not()
            .map(|o| o.unwrap_or_default());

        // Postfix operations (calls, field access, indexing)
        let call_args = expr
            .clone()
            .map_with_span(Spanned::new)
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let postfix = unary.then(
            choice((
                // Generic call with otherwise: Foo<T>(...) otherwise Label
                type_args
                    .clone()
                    .then(call_args.clone())
                    .then(otherwise_clause.clone())
                    .map(|((type_args, args), otherwise)| PostfixOp::GenericCall {
                        type_args,
                        args,
                        otherwise,
                    }),
                just(Token::Dot).ignore_then(ident()).map(PostfixOp::Field),
                expr.clone()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket))
                    .map(|e| PostfixOp::Index(Box::new(e))),
            ))
            .repeated(),
        )
        .foldl(|base, op| match op {
            PostfixOp::GenericCall {
                type_args,
                args,
                otherwise,
            } => Expr::Call {
                callee: Box::new(Spanned::new(base, 0..0)),
                type_args,
                args,
                otherwise,
            },
            PostfixOp::Field(field) => Expr::FieldAccess {
                object: Box::new(Spanned::new(base, 0..0)),
                field,
            },
            PostfixOp::Index(index) => Expr::Index {
                object: Box::new(Spanned::new(base, 0..0)),
                index: Box::new(Spanned::new(*index, 0..0)),
            },
        });

        // Binary operators - iterative precedence climbing
        let binary_op = filter_map(|span, tok| {
            get_binary_precedence(&tok)
                .map(|(prec, op)| (prec, op))
                .ok_or(Simple::expected_input_found(span, Vec::new(), Some(tok)))
        });

        let binary = postfix
            .clone()
            .then(binary_op.then(postfix).repeated())
            .map(|(first, rest)| {
                if rest.is_empty() {
                    first
                } else {
                    build_binary_expr(first, rest)
                }
            });

        // Ternary operator
        binary
            .clone()
            .then(
                just(Token::Question)
                    .ignore_then(expr.clone())
                    .then_ignore(just(Token::Colon))
                    .then(expr)
                    .or_not(),
            )
            .map(|(cond, ternary)| match ternary {
                Some((then_e, else_e)) => Expr::Ternary {
                    condition: Box::new(Spanned::new(cond, 0..0)),
                    then_expr: Box::new(Spanned::new(then_e, 0..0)),
                    else_expr: Box::new(Spanned::new(else_e, 0..0)),
                },
                None => cond,
            })
    })
}

enum PostfixOp {
    GenericCall {
        type_args: Vec<TypeExpr>,
        args: Vec<Spanned<Expr>>,
        otherwise: Vec<Ident>,
    },
    Field(Ident),
    Index(Box<Expr>),
}

/// Build a binary expression tree from a flat list, respecting operator precedence
fn build_binary_expr(first: Expr, rest: Vec<((Precedence, BinaryOp), Expr)>) -> Expr {
    if rest.is_empty() {
        return first;
    }

    let mut output: Vec<Expr> = vec![first];
    let mut ops: Vec<(Precedence, BinaryOp)> = Vec::new();

    for ((prec, op), rhs) in rest {
        while let Some(&(top_prec, top_op)) = ops.last() {
            if top_prec >= prec {
                ops.pop();
                let right = output.pop().unwrap();
                let left = output.pop().unwrap();
                output.push(Expr::Binary {
                    op: top_op,
                    left: Box::new(Spanned::new(left, 0..0)),
                    right: Box::new(Spanned::new(right, 0..0)),
                });
            } else {
                break;
            }
        }
        ops.push((prec, op));
        output.push(rhs);
    }

    while let Some((_, op)) = ops.pop() {
        let right = output.pop().unwrap();
        let left = output.pop().unwrap();
        output.push(Expr::Binary {
            op,
            left: Box::new(Spanned::new(left, 0..0)),
            right: Box::new(Spanned::new(right, 0..0)),
        });
    }

    output.pop().unwrap()
}
