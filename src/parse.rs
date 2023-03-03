use std::{error::Error, fmt, path::Path};

use crate::{
    ast::*,
    lex::{Simple::*, *},
};

#[derive(Debug)]
pub enum ParseError {
    Lex(LexError),
    Expected(Vec<Expectation>, Option<Box<Sp<Token>>>),
}

#[derive(Debug)]
pub enum Expectation {
    Ident,
    Block,
    Type,
    Expr,
    Pattern,
    Eof,
    FunctionBody,
    Simple(Simple),
    Keyword(Keyword),
}

impl From<Simple> for Expectation {
    fn from(simple: Simple) -> Self {
        Expectation::Simple(simple)
    }
}

impl From<Keyword> for Expectation {
    fn from(keyword: Keyword) -> Self {
        Expectation::Keyword(keyword)
    }
}

impl fmt::Display for Expectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expectation::Ident => write!(f, "identifier"),
            Expectation::Block => write!(f, "block"),
            Expectation::Type => write!(f, "type"),
            Expectation::Expr => write!(f, "expression"),
            Expectation::Pattern => write!(f, "pattern"),
            Expectation::Eof => write!(f, "end of file"),
            Expectation::FunctionBody => write!(f, "function body"),
            Expectation::Simple(s) => write!(f, "{s}"),
            Expectation::Keyword(k) => write!(f, "{k}"),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Lex(e) => write!(f, "{e}"),
            ParseError::Expected(exps, found) => {
                write!(f, "expected ")?;
                for (i, exp) in exps.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "`{exp}`")?;
                }
                if let Some(found) = found {
                    write!(f, ", found `{}`", found.value)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for ParseError {}

pub type ParseResult<T = ()> = Result<T, Sp<ParseError>>;

pub fn parse(input: &str, path: &Path) -> (Vec<Item>, Vec<Sp<ParseError>>) {
    let tokens = match lex(input, path) {
        Ok(tokens) => tokens,
        Err(e) => return (Vec::new(), vec![e.map(ParseError::Lex)]),
    };
    let mut items = Vec::new();
    let mut parser = Parser {
        tokens,
        index: 0,
        errors: Vec::new(),
    };
    loop {
        match parser.try_item() {
            Ok(Some(item)) => items.push(item),
            Ok(None) => break,
            Err(e) => parser.errors.push(e),
        }
    }
    parser.next_token_map::<()>(|_| None);
    if parser.index != parser.tokens.len() {
        parser.errors.push(parser.expected([Expectation::Eof]));
    }
    (items, parser.errors)
}

struct Parser {
    tokens: Vec<Sp<crate::lex::Token>>,
    index: usize,
    errors: Vec<Sp<ParseError>>,
}

#[allow(unused)]
const PARENS: (Simple, Simple) = (OpenParen, CloseParen);
const BRACKETS: (Simple, Simple) = (OpenBracket, CloseBracket);
#[allow(unused)]
const CURLIES: (Simple, Simple) = (OpenCurly, CloseCurly);

impl Parser {
    fn next_token_map<'a, T: 'a>(
        &'a mut self,
        f: impl FnOnce(&'a Token) -> Option<T>,
    ) -> Option<Sp<T>> {
        while let Some(token) = self.tokens.get(self.index) {
            if matches!(token.value, Token::Comment(_)) {
                self.index += 1;
            } else if let Some(value) = f(&token.value) {
                self.index += 1;
                return Some(token.span.clone().sp(value));
            } else {
                return None;
            }
        }
        None
    }
    fn try_exact(&mut self, token: impl Into<Token>) -> Option<Span> {
        let token = token.into();
        self.next_token_map(|t| (t == &token).then_some(()))
            .map(|t| t.span)
    }
    fn last_span(&self) -> Span {
        if let Some(token) = self.tokens.get(self.index) {
            token.span.clone()
        } else {
            let mut span = self.tokens.last().unwrap().span.clone();
            span.start = span.end;
            if self.tokens.len() > span.end.pos {
                span.end.pos += 1;
                span.end.col += 1;
            }
            span
        }
    }
    fn expect<T>(&mut self, val: T) -> ParseResult<Span>
    where
        T: Copy + Into<Token> + Into<Expectation>,
    {
        self.try_exact(val).ok_or_else(|| self.expected([val]))
    }
    fn expected<I: Into<Expectation>>(
        &self,
        expectations: impl IntoIterator<Item = I>,
    ) -> Sp<ParseError> {
        self.last_span().sp(ParseError::Expected(
            expectations.into_iter().map(Into::into).collect(),
            self.tokens.get(self.index).cloned().map(Box::new),
        ))
    }
    #[allow(unused)]
    fn expected_continue<I: Into<Expectation>>(
        &mut self,
        expectations: impl IntoIterator<Item = I>,
    ) {
        let err = self.last_span().sp(ParseError::Expected(
            expectations.into_iter().map(Into::into).collect(),
            None,
        ));
        self.errors.push(err);
    }
    fn try_item(&mut self) -> ParseResult<Option<Item>> {
        let item = if let Some(def) = self.try_function_def()? {
            Item::FunctionDef(def)
        } else if let Some(binding) = self.try_binding()? {
            Item::Binding(binding)
        } else if let Some(expr) = self.try_expr()? {
            Item::Expr(expr)
        } else {
            return Ok(None);
        };
        self.expect(SemiColon)?;
        Ok(Some(item))
    }
    fn doc_comment(&mut self) -> Option<Sp<String>> {
        let mut doc: Option<Sp<String>> = None;
        while let Some(line) = self.next_token_map(|token| {
            if let Token::DocComment(line) = token {
                Some(line)
            } else {
                None
            }
        }) {
            if let Some(doc) = &mut doc {
                doc.value.push_str(line.value);
                doc.span.end = line.span.end;
            } else {
                doc = Some(line.cloned());
            }
        }
        doc
    }
    fn try_function_def(&mut self) -> ParseResult<Option<FunctionDef>> {
        // Documentation comments
        let doc = self.doc_comment();
        // Keyword
        if self.try_exact(Keyword::Fn).is_none() {
            return Ok(None);
        };
        // Name
        let name = self.ident()?;
        // Parameters
        let mut params = Vec::new();
        while let Some(param) = self.try_ident() {
            params.push(param);
        }
        self.expect(Colon)?;
        // Body
        let body = self.block()?;
        let id = FunctionId::Named(name.value);
        Ok(Some(FunctionDef {
            doc,
            name,
            func: Func { id, params, body },
        }))
    }
    fn block(&mut self) -> ParseResult<Block> {
        let mut bindings = Vec::new();
        while let Some(binding) = self.try_binding()? {
            bindings.push(binding);
            self.expect(SemiColon)?;
        }
        let ret = self.expr()?;
        Ok(Block {
            bindings,
            expr: ret,
        })
    }
    fn try_binding(&mut self) -> ParseResult<Option<Binding>> {
        // Let
        if self.try_exact(Keyword::Let).is_none() {
            return Ok(None);
        };
        // Pattern
        let pattern = self.pattern()?;
        // Expression
        self.expect(Equal)?;
        let expr = self.expr()?;
        Ok(Some(Binding { pattern, expr }))
    }
    fn try_pattern(&mut self) -> ParseResult<Option<Sp<Pattern>>> {
        Ok(Some(if let Some(ident) = self.try_ident() {
            ident.map(Pattern::Ident)
        } else if let Some(items) = self.try_surrounded_list(BRACKETS, Self::try_pattern)? {
            items.map(Pattern::List)
        } else {
            return Ok(None);
        }))
    }
    fn pattern(&mut self) -> ParseResult<Sp<Pattern>> {
        self.try_pattern()?
            .ok_or_else(|| self.expected([Expectation::Pattern]))
    }
    fn try_ident(&mut self) -> Option<Sp<Ident>> {
        self.next_token_map(|token| token.as_ident().copied())
    }
    fn ident(&mut self) -> ParseResult<Sp<Ident>> {
        self.try_ident()
            .ok_or_else(|| self.expected([Expectation::Ident]))
    }
    fn try_surrounded_list<T>(
        &mut self,
        (open, close): (Simple, Simple),
        item: impl Fn(&mut Self) -> ParseResult<Option<T>>,
    ) -> ParseResult<Option<Sp<Vec<T>>>> {
        let Some(start) = self.try_exact(open) else {
            return Ok(None);
        };
        let mut items = Vec::new();
        while let Some(item) = item(self)? {
            items.push(item);
            if self.try_exact(Comma).is_none() {
                break;
            }
        }
        let end = self.expect(close)?;
        let span = start.merge(end);
        Ok(Some(span.sp(items)))
    }
}

struct BinExprDef<'a> {
    ops: &'a [(Token, BinOp)],
    child: Option<&'a Self>,
}

static BIN_EXPR_RANGE: BinExprDef = BinExprDef {
    ops: &[(Token::Simple(Elipses), BinOp::RangeEx)],
    child: Some(&BinExprDef {
        ops: &[
            (Token::Simple(Equal), BinOp::Eq),
            (Token::Simple(NotEqual), BinOp::Ne),
            (Token::Simple(Less), BinOp::Lt),
            (Token::Simple(LessEqual), BinOp::Le),
            (Token::Simple(Greater), BinOp::Gt),
            (Token::Simple(GreaterEqual), BinOp::Ge),
        ],
        child: Some(&BinExprDef {
            ops: &[
                (Token::Simple(Plus), BinOp::Add),
                (Token::Simple(Minus), BinOp::Sub),
            ],
            child: Some(&BinExprDef {
                ops: &[
                    (Token::Simple(Star), BinOp::Mul),
                    (Token::Simple(Slash), BinOp::Div),
                ],
                child: None,
            }),
        }),
    }),
};

impl Parser {
    fn try_expr(&mut self) -> ParseResult<Option<Sp<Expr>>> {
        self.try_or_expr()
    }
    fn expr(&mut self) -> ParseResult<Sp<Expr>> {
        self.expect_expr(Self::try_expr)
    }
    fn expect_expr(
        &mut self,
        f: impl FnOnce(&mut Self) -> ParseResult<Option<Sp<Expr>>>,
    ) -> ParseResult<Sp<Expr>> {
        f(self)?.ok_or_else(|| self.expected([Expectation::Expr]))
    }
    fn try_or_expr(&mut self) -> ParseResult<Option<Sp<Expr>>> {
        let Some(left) = self.try_and_expr()? else {
            return Ok(None);
        };
        let Some(op_span) = self.try_exact(Keyword::Or) else {
            return Ok(Some(left));
        };
        let op = op_span.sp(LogicOp::Or);
        let right = self.expect_expr(Self::try_and_expr)?;
        let span = left.span.clone().merge(right.span.clone());
        Ok(Some(span.sp(Expr::Logic(Box::new(LogicExpr {
            op,
            left,
            right,
        })))))
    }
    fn try_and_expr(&mut self) -> ParseResult<Option<Sp<Expr>>> {
        let Some(left) = self.try_bin_expr()? else {
            return Ok(None);
        };
        let Some(op_span) = self.try_exact(Keyword::And) else {
            return Ok(Some(left));
        };
        let op = op_span.sp(LogicOp::And);
        let right = self.expect_expr(Self::try_bin_expr)?;
        let span = left.span.clone().merge(right.span.clone());
        Ok(Some(span.sp(Expr::Logic(Box::new(LogicExpr {
            op,
            left,
            right,
        })))))
    }
    fn try_bin_expr(&mut self) -> ParseResult<Option<Sp<Expr>>> {
        self.try_bin_expr_def(&BIN_EXPR_RANGE)
    }
    fn try_bin_expr_def(&mut self, def: &BinExprDef) -> ParseResult<Option<Sp<Expr>>> {
        let leaf = |this: &mut Self| {
            if let Some(child) = def.child {
                this.try_bin_expr_def(child)
            } else {
                this.try_call()
            }
        };
        let Some(mut expr) = leaf(self)? else{
            return Ok(None);
        };
        'rhs: loop {
            for (token, op) in def.ops {
                if let Some(op_span) = self.try_exact(token.clone()) {
                    let op = op_span.sp(*op);
                    let right = self.expect_expr(leaf)?;
                    let span = expr.span.clone().merge(right.span.clone());
                    expr = span.sp(Expr::Bin(Box::new(BinExpr {
                        left: expr,
                        op,
                        right,
                    })));
                    continue 'rhs;
                }
            }
            break;
        }
        Ok(Some(expr))
    }
    fn try_call(&mut self) -> ParseResult<Option<Sp<Expr>>> {
        let Some(func) = self.try_term()? else {
            return Ok(None);
        };
        let mut args = Vec::new();
        while let Some(arg) = self.try_term()? {
            args.push(arg);
        }
        if args.is_empty() {
            return Ok(Some(func));
        }
        let start = func.span.clone();
        let end = args.last().map(|a| a.span.clone()).unwrap_or(start.clone());
        let span = start.merge(end);
        Ok(Some(span.sp(Expr::Call(Box::new(CallExpr { func, args })))))
    }
    fn try_term(&mut self) -> ParseResult<Option<Sp<Expr>>> {
        Ok(Some(if let Some(ident) = self.try_ident() {
            ident.map(Expr::Ident)
        } else if let Some(i) = self.next_token_map(Token::as_int) {
            i.map(Into::into).map(Expr::Int)
        } else if let Some(r) = self.next_token_map(Token::as_real) {
            r.map(Into::into).map(Expr::Real)
        } else if let Some(span) = self.try_exact(Keyword::True) {
            span.sp(Expr::Bool(true))
        } else if let Some(span) = self.try_exact(Keyword::False) {
            span.sp(Expr::Bool(false))
        } else if let Some(start) = self.try_exact(OpenParen) {
            let inner = self.try_expr()?;
            let end = self.expect(CloseParen)?;
            let span = start.merge(end);
            if let Some(expr) = inner {
                expr.map(Box::new).map(Expr::Parened)
            } else {
                span.sp(Expr::Unit)
            }
        } else if let Some(items) = self.try_surrounded_list(BRACKETS, Self::try_expr)? {
            items.map(Expr::List)
        } else if let Some(if_else) = self.try_if()? {
            if_else.map(Box::new).map(Expr::If)
        } else {
            return Ok(None);
        }))
    }
    fn try_if(&mut self) -> ParseResult<Option<Sp<IfExpr>>> {
        let Some(if_span) = self.try_exact(Keyword::If) else {
            return Ok(None);
        };
        let cond = self.expr()?;
        self.expect(Keyword::Then)?;
        let if_true = self.block()?;
        self.expect(Keyword::Else)?;
        let if_false = self.block()?;
        let span = if_span.merge(if_false.expr.span.clone());
        Ok(Some(span.sp(IfExpr {
            cond,
            if_true,
            if_false,
        })))
    }
}
