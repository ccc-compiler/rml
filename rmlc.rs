//! rmlc — Rust-ML 編譯器 (RML → Rust)，以 Rust 撰寫，不依賴任何 crate。
//!
//! 建置：  rustc -O rmlc.rs -o rmlc
//! 使用：  ./rmlc prog.rml [-o out.rs] [--run] [--stdout]
//!
//! 流程：lex → parse → HM 型別推論 → 所有權(move)檢查 → Rust 程式碼產生
#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// ════════════════════════════════════════════════════════════════
// 錯誤
// ════════════════════════════════════════════════════════════════
struct RmlError {
    msg: String,
    line: usize,
}
type R<T> = Result<T, RmlError>;
fn err<T>(msg: impl Into<String>, line: usize) -> R<T> {
    Err(RmlError { msg: msg.into(), line })
}

// ════════════════════════════════════════════════════════════════
// 詞法分析
// ════════════════════════════════════════════════════════════════
#[derive(Clone, Copy, PartialEq, Debug)]
enum K { Int, Float, Str, Id, Ctor, TyVar, Kw, Sym, Eof }

#[derive(Clone)]
struct Tok { k: K, v: String, line: usize }

const KEYWORDS: &[&str] = &["let", "rec", "in", "and", "fun", "if", "then", "else", "match", "with",
    "type", "of", "true", "false", "mut", "not", "mod"];
const SYMS: &[&str] = &["->", "::", ":=", "<-", "<>", "<=", ">=", "&&", "||", "+.", "-.", "*.", "/.",
    "(", ")", "[", "]", ",", ";", "|", ":", "*", "+", "-", "/", "^", "=", "<", ">", "!", "&"];

fn starts(cs: &[char], i: usize, s: &str) -> bool {
    s.chars().enumerate().all(|(k, c)| cs.get(i + k) == Some(&c))
}

fn lex(src: &str) -> R<Vec<Tok>> {
    let cs: Vec<char> = src.chars().collect();
    let n = cs.len();
    let (mut i, mut line) = (0usize, 1usize);
    let mut toks = Vec::new();
    while i < n {
        let c = cs[i];
        if c == '\n' { line += 1; i += 1; continue; }
        if c == ' ' || c == '\t' || c == '\r' { i += 1; continue; }
        if starts(&cs, i, "(*") {
            let mut depth = 1; i += 2;
            while i < n && depth > 0 {
                if starts(&cs, i, "(*") { depth += 1; i += 2; }
                else if starts(&cs, i, "*)") { depth -= 1; i += 2; }
                else { if cs[i] == '\n' { line += 1; } i += 1; }
            }
            if depth > 0 { return err("註解沒有結束", line); }
            continue;
        }
        if c.is_ascii_digit() {
            let st = i;
            let mut is_f = false;
            while i < n && (cs[i].is_ascii_digit() || cs[i] == '_') { i += 1; }
            if i < n && cs[i] == '.' {
                is_f = true; i += 1;
                while i < n && (cs[i].is_ascii_digit() || cs[i] == '_') { i += 1; }
            }
            if i < n && (cs[i] == 'e' || cs[i] == 'E') {
                let mut j = i + 1;
                if j < n && (cs[j] == '+' || cs[j] == '-') { j += 1; }
                if j < n && cs[j].is_ascii_digit() {
                    is_f = true; i = j;
                    while i < n && cs[i].is_ascii_digit() { i += 1; }
                }
            }
            let txt: String = cs[st..i].iter().filter(|&&c| c != '_').collect();
            toks.push(Tok { k: if is_f { K::Float } else { K::Int }, v: txt, line });
            continue;
        }
        if c == '"' {
            i += 1;
            let mut buf = String::new();
            while i < n && cs[i] != '"' {
                if cs[i] == '\\' {
                    i += 1;
                    let e = match cs.get(i) {
                        Some('n') => '\n', Some('t') => '\t', Some('r') => '\r',
                        Some('\\') => '\\', Some('"') => '"', Some('0') => '\0',
                        _ => return err("不合法的跳脫字元", line),
                    };
                    buf.push(e);
                } else {
                    if cs[i] == '\n' { line += 1; }
                    buf.push(cs[i]);
                }
                i += 1;
            }
            if i >= n { return err("字串沒有結束", line); }
            i += 1;
            toks.push(Tok { k: K::Str, v: buf, line });
            continue;
        }
        let is_id_start = |c: char| c.is_ascii_alphabetic() || c == '_';
        let is_id = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '\'';
        if c == '\'' && i + 1 < n && cs[i + 1].is_ascii_alphabetic() {
            let st = i + 1; i = st;
            while i < n && is_id(cs[i]) { i += 1; }
            toks.push(Tok { k: K::TyVar, v: cs[st..i].iter().collect(), line });
            continue;
        }
        if is_id_start(c) {
            let st = i;
            while i < n && is_id(cs[i]) { i += 1; }
            let w: String = cs[st..i].iter().collect();
            let k = if KEYWORDS.contains(&w.as_str()) { K::Kw }
                    else if w.chars().next().unwrap().is_ascii_uppercase() { K::Ctor }
                    else { K::Id };
            toks.push(Tok { k, v: w, line });
            continue;
        }
        match SYMS.iter().find(|s| starts(&cs, i, s)) {
            Some(s) => { toks.push(Tok { k: K::Sym, v: s.to_string(), line }); i += s.len(); }
            None => return err(format!("無法辨識的字元 {:?}", c), line),
        }
    }
    toks.push(Tok { k: K::Eof, v: "<eof>".into(), line });
    Ok(toks)
}

// ════════════════════════════════════════════════════════════════
// 型別 (內部表示)
// ════════════════════════════════════════════════════════════════
#[derive(Clone, Debug)]
enum Type {
    Var(usize),
    Con(String, Vec<Type>),
    Tuple(Vec<Type>),
    Fn(Vec<Type>, Box<Type>),
    Ref(Box<Type>, bool),
}
fn tcon(n: &str) -> Type { Type::Con(n.to_string(), vec![]) }
fn t_int() -> Type { tcon("int") }
fn t_float() -> Type { tcon("float") }
fn t_bool() -> Type { tcon("bool") }
fn t_unit() -> Type { tcon("unit") }
fn t_string() -> Type { tcon("string") }
const PRIMS: &[&str] = &["int", "float", "bool", "unit", "string"];

fn is_con(t: &Type, name: &str) -> bool {
    matches!(t, Type::Con(n, a) if n == name && a.is_empty())
}

// 型別語法樹 (來自使用者註記)
#[derive(Clone)]
enum TyAst {
    Var(String),
    Con(String, Vec<TyAst>),
    Tuple(Vec<TyAst>),
    Fn(Vec<TyAst>, Box<TyAst>),
    Ref(Box<TyAst>, bool),
    Args(Vec<TyAst>),
}

// ════════════════════════════════════════════════════════════════
// AST
// ════════════════════════════════════════════════════════════════
#[derive(Clone, Copy, PartialEq, Debug)]
enum Boxed { No, Own, Ref, Mut }

struct VarInfo { id: usize, name: String, ty: Type, mutable: bool, boxed: Boxed }
struct GlobalFn { name: String, scheme: RefCell<Scheme>, nparams: usize }
struct Builtin { name: &'static str, scheme: Scheme, borrow: Vec<bool> }

#[derive(Clone)]
enum VarRef { Local(Rc<VarInfo>), Global(Rc<GlobalFn>), Builtin(Rc<Builtin>) }

struct CtorInfo { name: String, tname: String, fields: Vec<TyAst>, boxed: Vec<bool>, ftypes: Vec<Type> }

struct Param { name: String, mutable: bool, ty: Option<TyAst>, line: usize }

struct Expr { line: usize, ty: Type, kind: EK }
enum EK {
    Int(String), Float(String), Str(String), Bool(bool), Unit,
    Var { name: String, var: Option<VarRef> },
    Ctor { name: String, arg: Option<Box<Expr>>, args: Vec<Expr>, info: Option<Rc<CtorInfo>> },
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    App { f: Box<Expr>, args: Vec<Expr>, builtin: Option<Rc<Builtin>> },
    Bin(String, Box<Expr>, Box<Expr>),
    Un(String, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    Seq(Box<Expr>, Box<Expr>),
    Let { pat: Pat, val: Box<Expr>, body: Box<Expr> },
    Match(Box<Expr>, Vec<(Pat, Expr)>),
    Fun { params: Vec<Param>, body: Box<Expr>, pinfos: Vec<Rc<VarInfo>> },
    Ref(Box<Expr>, bool),
    Deref(Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    SetVar { name: String, e: Box<Expr>, var: Option<Rc<VarInfo>> },
    Ann(Box<Expr>, TyAst),
}

struct Pat { line: usize, kind: PK }
enum PK {
    Wild,
    Var { name: String, mutable: bool, var: Option<Rc<VarInfo>> },
    Lit { int: bool, v: String },
    Unit,
    Tuple(Vec<Pat>),
    Ctor { name: String, arg: Option<Box<Pat>>, args: Option<Vec<Pat>>, info: Option<Rc<CtorInfo>> },
}

struct TypeDef { name: String, params: Vec<String>, ctors: Vec<(String, Vec<TyAst>, usize)>, line: usize }

struct FunDef {
    line: usize, name: String, params: Vec<Param>, ret: Option<TyAst>, body: Expr, rec: bool,
    // 推論後填入
    ptys: Vec<Type>, rty: Option<Type>, gfn: Option<Rc<GlobalFn>>,
    pinfos: Vec<Rc<VarInfo>>, reqs: Vec<(Type, &'static str)>,
}

enum Decl { Types(Vec<TypeDef>), Funs(Vec<FunDef>) }

fn ex(line: usize, kind: EK) -> Expr { Expr { line, ty: t_unit(), kind } }
fn pt(line: usize, kind: PK) -> Pat { Pat { line, kind } }

// ════════════════════════════════════════════════════════════════
// 語法分析
// ════════════════════════════════════════════════════════════════
struct Parser { t: Vec<Tok>, i: usize }

fn binop_info(op: &str) -> Option<(u8, bool)> {  // (優先權, 右結合?)
    Some(match op {
        ":=" | "<-" => (1, true), "||" => (2, true), "&&" => (3, true),
        "=" | "<>" | "<" | ">" | "<=" | ">=" => (4, false),
        "::" => (5, true), "^" => (6, true),
        "+" | "-" | "+." | "-." => (7, false),
        "*" | "/" | "*." | "/." | "mod" => (8, false),
        _ => return None,
    })
}

impl Parser {
    fn new(t: Vec<Tok>) -> Self { Parser { t, i: 0 } }
    fn peek(&self, o: usize) -> &Tok { &self.t[(self.i + o).min(self.t.len() - 1)] }
    fn next(&mut self) -> Tok {
        let t = self.t[self.i].clone();
        if t.k != K::Eof { self.i += 1; }
        t
    }
    fn at_o(&self, v: &str, o: usize) -> bool {
        let t = self.peek(o);
        (t.k == K::Kw || t.k == K::Sym) && t.v == v
    }
    fn at(&self, v: &str) -> bool { self.at_o(v, 0) }
    fn accept(&mut self, v: &str) -> bool { if self.at(v) { self.next(); true } else { false } }
    fn expect(&mut self, v: &str) -> R<Tok> {
        if !self.at(v) {
            let t = self.peek(0);
            return err(format!("語法錯誤：預期 `{}`，但看到 `{}`", v, t.v), t.line);
        }
        Ok(self.next())
    }
    fn expect_kind(&mut self, k: K, what: &str) -> R<Tok> {
        let t = self.peek(0);
        if t.k != k { return err(format!("語法錯誤：預期{}，但看到 `{}`", what, t.v), t.line); }
        Ok(self.next())
    }

    // --- 型別
    fn parse_ty(&mut self) -> R<TyAst> {
        let t = self.parse_ty_tuple()?;
        if self.accept("->") {
            let r = self.parse_ty()?;
            return Ok(match r {
                TyAst::Fn(mut ps, ret) => { ps.insert(0, t); TyAst::Fn(ps, ret) }
                r => TyAst::Fn(vec![t], Box::new(r)),
            });
        }
        Ok(t)
    }
    fn parse_ty_tuple(&mut self) -> R<TyAst> {
        let mut items = vec![self.parse_ty_post()?];
        while self.accept("*") { items.push(self.parse_ty_post()?); }
        Ok(if items.len() == 1 { items.pop().unwrap() } else { TyAst::Tuple(items) })
    }
    fn parse_ty_post(&mut self) -> R<TyAst> {
        let mut base = self.parse_ty_atom()?;
        while self.peek(0).k == K::Id {
            let name = self.next().v;
            base = match base { TyAst::Args(a) => TyAst::Con(name, a), b => TyAst::Con(name, vec![b]) };
        }
        if let TyAst::Args(_) = base { return err("型別參數列表後面需要型別建構子", self.peek(0).line); }
        Ok(base)
    }
    fn parse_ty_atom(&mut self) -> R<TyAst> {
        let t = self.peek(0).clone();
        if t.k == K::TyVar { self.next(); return Ok(TyAst::Var(t.v)); }
        if t.k == K::Id { self.next(); return Ok(TyAst::Con(t.v, vec![])); }
        if self.accept("&") {
            let m = self.accept("mut");
            return Ok(TyAst::Ref(Box::new(self.parse_ty_post()?), m));
        }
        if self.accept("(") {
            let a = self.parse_ty()?;
            if self.at(",") {
                let mut args = vec![a];
                while self.accept(",") { args.push(self.parse_ty()?); }
                self.expect(")")?;
                return Ok(TyAst::Args(args));
            }
            self.expect(")")?;
            return Ok(a);
        }
        err(format!("語法錯誤：預期型別，但看到 `{}`", t.v), t.line)
    }

    // --- 程式
    fn parse_program(&mut self) -> R<Vec<Decl>> {
        let mut decls = Vec::new();
        while self.peek(0).k != K::Eof {
            if self.at("type") { decls.push(Decl::Types(self.parse_typedecl()?)); }
            else if self.at("let") { decls.push(Decl::Funs(self.parse_fundecls()?)); }
            else {
                let t = self.peek(0);
                return err(format!("頂層只能是 `type` 或 `let`，但看到 `{}`", t.v), t.line);
            }
        }
        Ok(decls)
    }

    fn parse_typedecl(&mut self) -> R<Vec<TypeDef>> {
        self.expect("type")?;
        let mut defs = Vec::new();
        loop {
            let line = self.peek(0).line;
            let mut params = Vec::new();
            if self.peek(0).k == K::TyVar { params.push(self.next().v); }
            else if self.accept("(") {
                params.push(self.expect_kind(K::TyVar, "型別變數")?.v);
                while self.accept(",") { params.push(self.expect_kind(K::TyVar, "型別變數")?.v); }
                self.expect(")")?;
            }
            let name = self.expect_kind(K::Id, "型別名稱")?.v;
            self.expect("=")?;
            self.accept("|");
            let mut ctors = Vec::new();
            loop {
                let c = self.expect_kind(K::Ctor, "建構子名稱")?;
                let mut fields = Vec::new();
                if self.accept("of") {
                    fields.push(self.parse_ty_post()?);
                    while self.accept("*") { fields.push(self.parse_ty_post()?); }
                }
                ctors.push((c.v, fields, c.line));
                if !self.accept("|") { break; }
            }
            defs.push(TypeDef { name, params, ctors, line });
            if !self.accept("and") { break; }
        }
        Ok(defs)
    }

    fn parse_params(&mut self) -> R<Vec<Param>> {
        let mut ps = Vec::new();
        loop {
            let t = self.peek(0).clone();
            if t.k == K::Id {
                self.next();
                ps.push(Param { name: t.v, mutable: false, ty: None, line: t.line });
            } else if self.at("(") {
                if self.at_o(")", 1) {
                    self.next(); self.next();
                    ps.push(Param { name: "_unit".into(), mutable: false,
                                    ty: Some(TyAst::Con("unit".into(), vec![])), line: t.line });
                } else {
                    self.next();
                    let m = self.accept("mut");
                    let name = self.expect_kind(K::Id, "參數名稱")?.v;
                    let ty = if self.accept(":") { Some(self.parse_ty()?) } else { None };
                    self.expect(")")?;
                    ps.push(Param { name, mutable: m, ty, line: t.line });
                }
            } else { break; }
        }
        Ok(ps)
    }

    fn parse_fundecls(&mut self) -> R<Vec<FunDef>> {
        self.expect("let")?;
        let rec = self.accept("rec");
        let mut defs = Vec::new();
        loop {
            let nt = self.expect_kind(K::Id, "函式名稱")?;
            let params = self.parse_params()?;
            if params.is_empty() {
                return err(format!("頂層 `{}` 必須是函式（至少一個參數，沒有參數請用 `()`）", nt.v), nt.line);
            }
            let ret = if self.accept(":") { Some(self.parse_ty()?) } else { None };
            self.expect("=")?;
            let body = self.parse_seq()?;
            defs.push(FunDef { line: nt.line, name: nt.v, params, ret, body, rec,
                ptys: vec![], rty: None, gfn: None, pinfos: vec![], reqs: vec![] });
            if !self.accept("and") { break; }
        }
        Ok(defs)
    }

    // --- 運算式
    fn starts_atom(&self) -> bool {
        let t = self.peek(0);
        match t.k {
            K::Int | K::Float | K::Str | K::Id | K::Ctor => true,
            K::Kw => t.v == "true" || t.v == "false",
            K::Sym => matches!(t.v.as_str(), "(" | "[" | "&" | "!"),
            _ => false,
        }
    }
    fn starts_expr(&self) -> bool {
        if self.starts_atom() { return true; }
        let t = self.peek(0);
        match t.k {
            K::Kw => matches!(t.v.as_str(), "let" | "if" | "match" | "fun" | "not"),
            K::Sym => t.v == "-" || t.v == "-.",
            _ => false,
        }
    }

    fn parse_seq(&mut self) -> R<Expr> {
        let a = self.parse_expr()?;
        if self.at(";") {
            let line = self.next().line;
            let b = if self.starts_expr() { self.parse_seq()? } else { ex(line, EK::Unit) };
            return Ok(ex(line, EK::Seq(Box::new(a), Box::new(b))));
        }
        Ok(a)
    }

    fn parse_expr(&mut self) -> R<Expr> { self.parse_binop(1) }

    fn parse_binop(&mut self, minp: u8) -> R<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let t = self.peek(0).clone();
            if !(t.k == K::Sym || t.k == K::Kw) { break; }
            let (p, right_assoc) = match binop_info(&t.v) { Some(x) => x, None => break };
            if p < minp { break; }
            self.next();
            let right = self.parse_binop(if right_assoc { p } else { p + 1 })?;
            let line = t.line;
            left = match t.v.as_str() {
                "::" => ex(line, EK::Ctor { name: "Cons".into(),
                    arg: Some(Box::new(ex(line, EK::Tuple(vec![left, right])))), args: vec![], info: None }),
                ":=" => ex(line, EK::Assign(Box::new(left), Box::new(right))),
                "<-" => match left.kind {
                    EK::Var { name, .. } => ex(line, EK::SetVar { name, e: Box::new(right), var: None }),
                    _ => return err("`<-` 左邊必須是變數", line),
                },
                op => ex(line, EK::Bin(op.to_string(), Box::new(left), Box::new(right))),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> R<Expr> {
        let t = self.peek(0).clone();
        if self.at("-") || self.at("-.") {
            self.next();
            let e = self.parse_unary()?;
            let line = t.line;
            return Ok(match e.kind {
                EK::Int(v) if t.v == "-" => ex(line, EK::Int(format!("-{}", v.trim_start_matches('-')))),
                EK::Float(v) if t.v == "-." => ex(line, EK::Float(format!("-{}", v.trim_start_matches('-')))),
                kind => ex(line, EK::Un(if t.v == "-" { "neg" } else { "fneg" }.into(),
                                        Box::new(Expr { line: e.line, ty: e.ty, kind }))),
            });
        }
        if self.at("not") {
            self.next();
            let e = self.parse_unary()?;
            return Ok(ex(t.line, EK::Un("not".into(), Box::new(e))));
        }
        if self.at("let") { return self.parse_let(); }
        if self.at("if") {
            self.next();
            let c = self.parse_expr()?;
            self.expect("then")?;
            let a = self.parse_expr()?;
            let b = if self.accept("else") { Some(Box::new(self.parse_expr()?)) } else { None };
            return Ok(ex(t.line, EK::If(Box::new(c), Box::new(a), b)));
        }
        if self.at("match") {
            self.next();
            let e = self.parse_seq()?;
            self.expect("with")?;
            self.accept("|");
            let mut arms = Vec::new();
            loop {
                let p = self.parse_pat()?;
                self.expect("->")?;
                let body = self.parse_seq()?;
                arms.push((p, body));
                if !self.accept("|") { break; }
            }
            return Ok(ex(t.line, EK::Match(Box::new(e), arms)));
        }
        if self.at("fun") {
            self.next();
            let params = self.parse_params()?;
            if params.is_empty() { return err("`fun` 需要至少一個參數", t.line); }
            self.expect("->")?;
            let body = self.parse_seq()?;
            return Ok(ex(t.line, EK::Fun { params, body: Box::new(body), pinfos: vec![] }));
        }
        self.parse_app()
    }

    fn parse_let(&mut self) -> R<Expr> {
        let line = self.expect("let")?.line;
        if self.at("rec") { return err("區域 `let rec` 尚未支援；請把遞迴函式放到頂層", line); }
        let m = self.accept("mut");
        // let f x y = ... in   (區域函式語法糖)
        if self.peek(0).k == K::Id && !self.at_o("=", 1) && !self.at_o(":", 1) && !m {
            let name = self.next();
            let params = self.parse_params()?;
            if !params.is_empty() {
                let ret = if self.accept(":") { Some(self.parse_ty()?) } else { None };
                self.expect("=")?;
                let mut body = self.parse_seq()?;
                if let Some(r) = ret { body = ex(line, EK::Ann(Box::new(body), r)); }
                let val = ex(line, EK::Fun { params, body: Box::new(body), pinfos: vec![] });
                self.expect("in")?;
                let rest = self.parse_seq()?;
                return Ok(ex(line, EK::Let {
                    pat: pt(line, PK::Var { name: name.v, mutable: false, var: None }),
                    val: Box::new(val), body: Box::new(rest) }));
            }
            self.i -= 1;
        }
        let mut pat = self.parse_pat()?;
        if m {
            match &mut pat.kind {
                PK::Var { mutable, .. } => *mutable = true,
                _ => return err("`let mut` 之後必須是單一變數", line),
            }
        }
        let ann = if self.accept(":") { Some(self.parse_ty()?) } else { None };
        self.expect("=")?;
        let mut val = self.parse_seq()?;
        if let Some(a) = ann { val = ex(line, EK::Ann(Box::new(val), a)); }
        self.expect("in")?;
        let body = self.parse_seq()?;
        Ok(ex(line, EK::Let { pat, val: Box::new(val), body: Box::new(body) }))
    }

    fn parse_app(&mut self) -> R<Expr> {
        let mut head = self.parse_atom()?;
        if let EK::Ctor { arg, .. } = &mut head.kind {
            if arg.is_none() && self.starts_atom() { *arg = Some(Box::new(self.parse_atom()?)); }
            return Ok(head);
        }
        let mut args = Vec::new();
        while self.starts_atom() { args.push(self.parse_atom()?); }
        if args.is_empty() { return Ok(head); }
        let line = head.line;
        Ok(ex(line, EK::App { f: Box::new(head), args, builtin: None }))
    }

    fn parse_atom(&mut self) -> R<Expr> {
        let t = self.next();
        let line = t.line;
        match t.k {
            K::Int => return Ok(ex(line, EK::Int(t.v))),
            K::Float => return Ok(ex(line, EK::Float(t.v))),
            K::Str => return Ok(ex(line, EK::Str(t.v))),
            K::Id => {
                if t.v == "_" { return err("`_` 不能當作運算式", line); }
                return Ok(ex(line, EK::Var { name: t.v, var: None }));
            }
            K::Ctor => return Ok(ex(line, EK::Ctor { name: t.v, arg: None, args: vec![], info: None })),
            K::Kw if t.v == "true" || t.v == "false" => return Ok(ex(line, EK::Bool(t.v == "true"))),
            K::Sym => match t.v.as_str() {
                "(" => {
                    if self.accept(")") { return Ok(ex(line, EK::Unit)); }
                    let e = self.parse_seq()?;
                    if self.at(",") {
                        let mut items = vec![e];
                        while self.accept(",") { items.push(self.parse_expr()?); }
                        self.expect(")")?;
                        return Ok(ex(line, EK::Tuple(items)));
                    }
                    if self.accept(":") {
                        let ty = self.parse_ty()?;
                        self.expect(")")?;
                        return Ok(ex(line, EK::Ann(Box::new(e), ty)));
                    }
                    self.expect(")")?;
                    return Ok(e);
                }
                "[" => {
                    let mut items = Vec::new();
                    if !self.at("]") {
                        items.push(self.parse_expr()?);
                        while self.accept(";") {
                            if self.at("]") { break; }
                            items.push(self.parse_expr()?);
                        }
                    }
                    self.expect("]")?;
                    return Ok(ex(line, EK::List(items)));
                }
                "&" => {
                    let m = self.accept("mut");
                    let e = self.parse_atom()?;
                    return Ok(ex(line, EK::Ref(Box::new(e), m)));
                }
                "!" => {
                    let e = self.parse_atom()?;
                    return Ok(ex(line, EK::Deref(Box::new(e))));
                }
                _ => {}
            },
            _ => {}
        }
        err(format!("語法錯誤：非預期的 `{}`", t.v), line)
    }

    // --- 模式
    fn starts_pat_atom(&self) -> bool {
        let t = self.peek(0);
        match t.k {
            K::Int | K::Id | K::Ctor => true,
            K::Kw => t.v == "true" || t.v == "false",
            K::Sym => matches!(t.v.as_str(), "(" | "[" | "-"),
            _ => false,
        }
    }
    fn parse_pat(&mut self) -> R<Pat> {
        let p = self.parse_pat_app()?;
        if self.at("::") {
            let line = self.next().line;
            let r = self.parse_pat()?;
            return Ok(pt(line, PK::Ctor { name: "Cons".into(), arg: None, args: Some(vec![p, r]), info: None }));
        }
        Ok(p)
    }
    fn parse_pat_app(&mut self) -> R<Pat> {
        let t = self.peek(0).clone();
        if t.k == K::Ctor {
            self.next();
            let arg = if self.starts_pat_atom() { Some(Box::new(self.parse_pat_atom()?)) } else { None };
            return Ok(pt(t.line, PK::Ctor { name: t.v, arg, args: None, info: None }));
        }
        self.parse_pat_atom()
    }
    fn parse_pat_atom(&mut self) -> R<Pat> {
        let t = self.next();
        let line = t.line;
        match t.k {
            K::Id => {
                return Ok(if t.v == "_" { pt(line, PK::Wild) }
                          else { pt(line, PK::Var { name: t.v, mutable: false, var: None }) })
            }
            K::Int => return Ok(pt(line, PK::Lit { int: true, v: t.v })),
            K::Kw if t.v == "true" || t.v == "false" => return Ok(pt(line, PK::Lit { int: false, v: t.v })),
            K::Ctor => return Ok(pt(line, PK::Ctor { name: t.v, arg: None, args: None, info: None })),
            K::Sym => match t.v.as_str() {
                "-" if self.peek(0).k == K::Int => {
                    let v = self.next().v;
                    return Ok(pt(line, PK::Lit { int: true, v: format!("-{}", v) }));
                }
                "[" => {
                    self.expect("]")?;
                    return Ok(pt(line, PK::Ctor { name: "Nil".into(), arg: None, args: None, info: None }));
                }
                "(" => {
                    if self.accept(")") { return Ok(pt(line, PK::Unit)); }
                    let p = self.parse_pat()?;
                    if self.at(",") {
                        let mut items = vec![p];
                        while self.accept(",") { items.push(self.parse_pat()?); }
                        self.expect(")")?;
                        return Ok(pt(line, PK::Tuple(items)));
                    }
                    self.expect(")")?;
                    return Ok(p);
                }
                _ => {}
            },
            _ => {}
        }
        err(format!("語法錯誤：非預期的模式 `{}`", t.v), line)
    }
}

// ════════════════════════════════════════════════════════════════
// 型別推論 (Hindley–Milner)，同時記錄所有權所需資訊
// ════════════════════════════════════════════════════════════════
#[derive(Clone)]
struct Scheme { vars: Vec<usize>, ty: Type }

struct TypeInfo {
    name: String, params: Vec<String>, rname: String,
    ctors: Vec<Rc<CtorInfo>>, copy_ok: bool, pvars: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum Bm { N, R, M }

fn wrap(t: Type, m: Bm) -> Type {
    match m { Bm::N => t, Bm::R => Type::Ref(Box::new(t), false), Bm::M => Type::Ref(Box::new(t), true) }
}

const PRELUDE: &str = "
type 'a list = Nil | Cons of 'a * 'a list
type 'a option = None | Some of 'a
";
const RESERVED_TYPES: &[&str] = &["Option", "Result", "Box", "Vec", "String", "Some", "None", "Ok", "Err",
    "Self", "Fn", "Copy", "Clone", "Drop", "Default", "Eq", "Ord"];

struct Infer {
    subst: Vec<Option<Type>>,
    types: HashMap<String, TypeInfo>,
    ctors: HashMap<String, Rc<CtorInfo>>,
    globals: HashMap<String, Rc<GlobalFn>>,
    builtins: HashMap<&'static str, Rc<Builtin>>,
    scopes: Vec<HashMap<String, Rc<VarInfo>>>,
    reqs: Vec<(Type, &'static str)>,
    tvmap: HashMap<String, Type>,
    next_var: usize,
}

fn mentions(a: &TyAst, group: &HashSet<String>) -> bool {
    match a {
        TyAst::Con(n, args) => group.contains(n) || args.iter().any(|x| mentions(x, group)),
        TyAst::Tuple(xs) | TyAst::Args(xs) => xs.iter().any(|x| mentions(x, group)),
        TyAst::Ref(x, _) => mentions(x, group),
        TyAst::Fn(ps, r) => ps.iter().any(|x| mentions(x, group)) || mentions(r, group),
        TyAst::Var(_) => false,
    }
}
fn has_fn_or_ref(a: &TyAst) -> bool {
    match a {
        TyAst::Fn(..) | TyAst::Ref(..) => true,
        TyAst::Con(_, args) => args.iter().any(has_fn_or_ref),
        TyAst::Tuple(xs) | TyAst::Args(xs) => xs.iter().any(has_fn_or_ref),
        TyAst::Var(_) => false,
    }
}

impl Infer {
    fn new() -> Self {
        let mut s = Infer {
            subst: vec![], types: HashMap::new(), ctors: HashMap::new(), globals: HashMap::new(),
            builtins: HashMap::new(), scopes: vec![HashMap::new()], reqs: vec![],
            tvmap: HashMap::new(), next_var: 0,
        };
        s.make_builtins();
        s
    }

    fn fresh(&mut self) -> usize { self.subst.push(None); self.subst.len() - 1 }
    fn fresh_t(&mut self) -> Type { Type::Var(self.fresh()) }

    fn make_builtins(&mut self) {
        let a = self.fresh();
        let mut add = |name: &'static str, params: Vec<Type>, ret: Type, borrow: Option<Vec<bool>>, vars: Vec<usize>| {
            let borrow = borrow.unwrap_or_else(|| vec![false; params.len()]);
            let b = Builtin { name, scheme: Scheme { vars, ty: Type::Fn(params, Box::new(ret)) }, borrow };
            self.builtins.insert(name, Rc::new(b));
        };
        let (i, f, u, s) = (t_int(), t_float(), t_unit(), t_string());
        add("print_int", vec![i.clone()], u.clone(), None, vec![]);
        add("print_float", vec![f.clone()], u.clone(), None, vec![]);
        add("print_string", vec![s.clone()], u.clone(), Some(vec![true]), vec![]);
        add("print_endline", vec![s.clone()], u.clone(), Some(vec![true]), vec![]);
        add("print_newline", vec![u.clone()], u.clone(), None, vec![]);
        add("string_of_int", vec![i.clone()], s.clone(), None, vec![]);
        add("string_of_float", vec![f.clone()], s.clone(), None, vec![]);
        add("string_length", vec![s.clone()], i.clone(), Some(vec![true]), vec![]);
        add("float_of_int", vec![i.clone()], f.clone(), None, vec![]);
        add("int_of_float", vec![f.clone()], i.clone(), None, vec![]);
        add("sqrt", vec![f.clone()], f.clone(), None, vec![]);
        add("abs", vec![i.clone()], i.clone(), None, vec![]);
        add("min", vec![i.clone(), i.clone()], i.clone(), None, vec![]);
        add("max", vec![i.clone(), i.clone()], i.clone(), None, vec![]);
        add("clone", vec![Type::Var(a)], Type::Var(a), Some(vec![true]), vec![a]);
    }

    // --- 型別工具
    fn prune(&self, t: &Type) -> Type {
        let mut t = t.clone();
        loop {
            let next = match &t { Type::Var(i) => self.subst[*i].clone(), _ => None };
            match next { Some(x) => t = x, None => return t }
        }
    }

    fn show(&self, t: &Type) -> String {
        match self.prune(t) {
            Type::Var(i) => format!("'t{}", i),
            Type::Con(n, a) => {
                if a.is_empty() { n }
                else if a.len() == 1 { format!("{} {}", self.show(&a[0]), n) }
                else { format!("({}) {}", a.iter().map(|x| self.show(x)).collect::<Vec<_>>().join(", "), n) }
            }
            Type::Tuple(xs) => format!("({})", xs.iter().map(|x| self.show(x)).collect::<Vec<_>>().join(" * ")),
            Type::Fn(ps, r) => {
                let mut v: Vec<String> = ps.iter().map(|x| self.show(x)).collect();
                v.push(self.show(&r));
                format!("({})", v.join(" -> "))
            }
            Type::Ref(i, m) => format!("{}{}", if m { "&mut " } else { "&" }, self.show(&i)),
        }
    }

    fn occurs(&self, v: usize, t: &Type) -> bool {
        match self.prune(t) {
            Type::Var(i) => i == v,
            Type::Con(_, a) | Type::Tuple(a) => a.iter().any(|x| self.occurs(v, x)),
            Type::Fn(ps, r) => ps.iter().any(|x| self.occurs(v, x)) || self.occurs(v, &r),
            Type::Ref(i, _) => self.occurs(v, &i),
        }
    }

    fn unify(&mut self, a: &Type, b: &Type, line: usize) -> R<()> {
        let (a, b) = (self.prune(a), self.prune(b));
        let bad = |s: &Infer| err(format!("型別不符：預期 {}，實際是 {}", s.show(&b), s.show(&a)), line);
        match (&a, &b) {
            (Type::Var(i), Type::Var(j)) if i == j => Ok(()),
            (Type::Var(i), _) => {
                if self.occurs(*i, &b) { return err("型別錯誤：無限遞迴型別", line); }
                self.subst[*i] = Some(b.clone());
                Ok(())
            }
            (_, Type::Var(_)) => self.unify(&b, &a, line),
            (Type::Con(n1, a1), Type::Con(n2, a2)) => {
                if n1 != n2 || a1.len() != a2.len() { return bad(self); }
                for (x, y) in a1.iter().zip(a2.iter()) { self.unify(x, y, line)?; }
                Ok(())
            }
            (Type::Tuple(a1), Type::Tuple(a2)) => {
                if a1.len() != a2.len() { return bad(self); }
                for (x, y) in a1.iter().zip(a2.iter()) { self.unify(x, y, line)?; }
                Ok(())
            }
            (Type::Fn(p1, r1), Type::Fn(p2, r2)) => {
                if p1.len() != p2.len() { return bad(self); }
                for (x, y) in p1.iter().zip(p2.iter()) { self.unify(x, y, line)?; }
                self.unify(r1, r2, line)
            }
            (Type::Ref(i1, m1), Type::Ref(i2, m2)) => {
                if m1 != m2 { return bad(self); }
                self.unify(i1, i2, line)
            }
            _ => bad(self),
        }
    }

    fn subst_ty(&self, t: &Type, m: &HashMap<usize, Type>) -> Type {
        match self.prune(t) {
            Type::Var(i) => m.get(&i).cloned().unwrap_or(Type::Var(i)),
            Type::Con(n, a) => Type::Con(n, a.iter().map(|x| self.subst_ty(x, m)).collect()),
            Type::Tuple(a) => Type::Tuple(a.iter().map(|x| self.subst_ty(x, m)).collect()),
            Type::Fn(ps, r) => Type::Fn(ps.iter().map(|x| self.subst_ty(x, m)).collect(), Box::new(self.subst_ty(&r, m))),
            Type::Ref(i, mu) => Type::Ref(Box::new(self.subst_ty(&i, m)), mu),
        }
    }

    fn free_tvars(&self, t: &Type, acc: &mut Vec<usize>) {
        match self.prune(t) {
            Type::Var(i) => { if !acc.contains(&i) { acc.push(i); } }
            Type::Con(_, a) | Type::Tuple(a) => for x in &a { self.free_tvars(x, acc) },
            Type::Fn(ps, r) => { for x in &ps { self.free_tvars(x, acc) } self.free_tvars(&r, acc) }
            Type::Ref(i, _) => self.free_tvars(&i, acc),
        }
    }

    fn contains_ref(&self, t: &Type) -> bool {
        match self.prune(t) {
            Type::Ref(..) => true,
            Type::Con(_, a) | Type::Tuple(a) => a.iter().any(|x| self.contains_ref(x)),
            Type::Fn(ps, r) => ps.iter().any(|x| self.contains_ref(x)) || self.contains_ref(&r),
            Type::Var(_) => false,
        }
    }

    fn inst(&mut self, s: &Scheme) -> Type {
        if s.vars.is_empty() { return s.ty.clone(); }
        let mut m = HashMap::new();
        for &v in &s.vars { let f = self.fresh_t(); m.insert(v, f); }
        self.subst_ty(&s.ty, &m)
    }

    // --- 環境
    fn bind(&mut self, name: &str, ty: Type, mutable: bool, boxed: Boxed) -> Rc<VarInfo> {
        self.next_var += 1;
        let vi = Rc::new(VarInfo { id: self.next_var, name: name.to_string(), ty, mutable, boxed });
        self.scopes.last_mut().unwrap().insert(name.to_string(), vi.clone());
        vi
    }
    fn lookup(&self, name: &str, line: usize) -> R<VarRef> {
        for sc in self.scopes.iter().rev() {
            if let Some(v) = sc.get(name) { return Ok(VarRef::Local(v.clone())); }
        }
        if let Some(g) = self.globals.get(name) { return Ok(VarRef::Global(g.clone())); }
        if let Some(b) = self.builtins.get(name) { return Ok(VarRef::Builtin(b.clone())); }
        err(format!("未定義的變數 `{}`", name), line)
    }

    // --- 型別宣告
    fn conv(&mut self, ast: &TyAst, tvm: &mut HashMap<String, Type>, allow_new: bool, line: usize) -> R<Type> {
        Ok(match ast {
            TyAst::Var(n) => {
                if !tvm.contains_key(n) {
                    if !allow_new { return err(format!("未宣告的型別變數 '{}", n), line); }
                    let f = self.fresh_t();
                    tvm.insert(n.clone(), f);
                }
                tvm[n].clone()
            }
            TyAst::Tuple(xs) => {
                let mut v = vec![];
                for x in xs { v.push(self.conv(x, tvm, allow_new, line)?); }
                Type::Tuple(v)
            }
            TyAst::Fn(ps, r) => {
                let mut v = vec![];
                for x in ps { v.push(self.conv(x, tvm, allow_new, line)?); }
                Type::Fn(v, Box::new(self.conv(r, tvm, allow_new, line)?))
            }
            TyAst::Ref(x, m) => Type::Ref(Box::new(self.conv(x, tvm, allow_new, line)?), *m),
            TyAst::Args(_) => return err("內部錯誤：型別參數列表", line),
            TyAst::Con(name, args) => {
                let mut v = vec![];
                for x in args { v.push(self.conv(x, tvm, allow_new, line)?); }
                if PRIMS.contains(&name.as_str()) {
                    if !v.is_empty() { return err(format!("型別 `{}` 不接受型別參數", name), line); }
                    return Ok(tcon(name));
                }
                match self.types.get(name) {
                    None => return err(format!("未定義的型別 `{}`", name), line),
                    Some(ti) if ti.params.len() != v.len() =>
                        return err(format!("型別 `{}` 需要 {} 個型別參數", name, ti.params.len()), line),
                    _ => {}
                }
                Type::Con(name.clone(), v)
            }
        })
    }

    fn conv_fn(&mut self, ast: &TyAst, line: usize) -> R<Type> {
        let mut tvm = std::mem::take(&mut self.tvmap);
        let r = self.conv(ast, &mut tvm, true, line);
        self.tvmap = tvm;
        r
    }

    fn copy_f(&self, f: &TyAst) -> bool {
        match f {
            TyAst::Var(_) => true,
            TyAst::Tuple(xs) => xs.iter().all(|x| self.copy_f(x)),
            TyAst::Con(n, args) => {
                if matches!(n.as_str(), "int" | "float" | "bool" | "unit") { return true; }
                if n == "string" { return false; }
                self.types[n].copy_ok && args.iter().all(|x| self.copy_f(x))
            }
            _ => false,
        }
    }

    fn declare_types(&mut self, defs: &[TypeDef]) -> R<()> {
        for d in defs {
            if self.types.contains_key(&d.name) || PRIMS.contains(&d.name.as_str()) {
                return err(format!("型別 `{}` 重複定義", d.name), d.line);
            }
            let mut rn = d.name[..1].to_uppercase() + &d.name[1..];
            if RESERVED_TYPES.contains(&rn.as_str()) { rn.push('_'); }
            let pvars: Vec<usize> = d.params.iter().map(|_| self.fresh()).collect();
            self.types.insert(d.name.clone(), TypeInfo {
                name: d.name.clone(), params: d.params.clone(), rname: rn,
                ctors: vec![], copy_ok: true, pvars });
        }
        let group: HashSet<String> = defs.iter().map(|d| d.name.clone()).collect();
        for d in defs {
            let mut tvm: HashMap<String, Type> = HashMap::new();
            for (p, &v) in d.params.iter().zip(self.types[&d.name].pvars.iter()) { tvm.insert(p.clone(), Type::Var(v)); }
            for (cname, fields, line) in &d.ctors {
                if self.ctors.contains_key(cname) { return err(format!("建構子 `{}` 重複定義", cname), *line); }
                let (mut boxed, mut ftypes) = (vec![], vec![]);
                for f in fields {
                    if matches!(f, TyAst::Con(n, _) if group.contains(n)) { boxed.push(true); }
                    else {
                        if mentions(f, &group) {
                            return err(format!("建構子 `{}`：遞迴型別必須直接作為欄位（不能包在 tuple/參數裡）", cname), *line);
                        }
                        boxed.push(false);
                    }
                    if has_fn_or_ref(f) {
                        return err(format!("建構子 `{}`：資料型別欄位不支援函式或參考", cname), *line);
                    }
                    ftypes.push(self.conv(f, &mut tvm, false, *line)?);
                }
                let ci = Rc::new(CtorInfo { name: cname.clone(), tname: d.name.clone(),
                                            fields: fields.clone(), boxed, ftypes });
                self.types.get_mut(&d.name).unwrap().ctors.push(ci.clone());
                self.ctors.insert(cname.clone(), ci);
            }
        }
        // Copy 推導 (fixpoint)
        let mut changed = true;
        while changed {
            changed = false;
            for d in defs {
                if !self.types[&d.name].copy_ok { continue; }
                let ok = self.types[&d.name].ctors.iter().all(|c|
                    c.boxed.iter().zip(c.fields.iter()).all(|(b, f)| !*b && self.copy_f(f)));
                if !ok { self.types.get_mut(&d.name).unwrap().copy_ok = false; changed = true; }
            }
        }
        Ok(())
    }

    fn ctor_inst(&mut self, info: &CtorInfo) -> (Vec<Type>, Type) {
        let pvars = self.types[&info.tname].pvars.clone();
        let mut m = HashMap::new();
        for &v in &pvars { let f = self.fresh_t(); m.insert(v, f); }
        let fts = info.ftypes.iter().map(|t| self.subst_ty(t, &m)).collect();
        let res = Type::Con(info.tname.clone(), pvars.iter().map(|v| m[v].clone()).collect());
        (fts, res)
    }

    // --- 頂層函式群
    fn infer_group(&mut self, fds: &mut [FunDef]) -> R<()> {
        for fd in fds.iter() {
            if self.globals.contains_key(&fd.name) { return err(format!("函式 `{}` 重複定義", fd.name), fd.line); }
        }
        self.tvmap.clear();
        for fd in fds.iter_mut() {
            let mut ptys = vec![];
            for p in &fd.params {
                ptys.push(match &p.ty { Some(a) => self.conv_fn(a, p.line)?, None => self.fresh_t() });
            }
            let rty = match &fd.ret { Some(a) => self.conv_fn(a, fd.line)?, None => self.fresh_t() };
            let fty = Type::Fn(ptys.clone(), Box::new(rty.clone()));
            fd.gfn = Some(Rc::new(GlobalFn { name: fd.name.clone(),
                scheme: RefCell::new(Scheme { vars: vec![], ty: fty }), nparams: fd.params.len() }));
            fd.ptys = ptys;
            fd.rty = Some(rty);
        }
        if fds[0].rec {
            for fd in fds.iter() { self.globals.insert(fd.name.clone(), fd.gfn.clone().unwrap()); }
        }
        for fd in fds.iter_mut() {
            self.scopes = vec![HashMap::new()];
            self.reqs.clear();
            fd.pinfos.clear();
            for (p, t) in fd.params.iter().zip(fd.ptys.iter()) {
                let vi = self.bind(&p.name, t.clone(), p.mutable, Boxed::No);
                fd.pinfos.push(vi);
            }
            let bt = self.infer(&mut fd.body)?;
            let rty = fd.rty.clone().unwrap();
            self.unify(&bt, &rty, fd.body.line)?;
            fd.reqs = std::mem::take(&mut self.reqs);
        }
        for fd in fds.iter() {
            let g = fd.gfn.clone().unwrap();
            let fty = g.scheme.borrow().ty.clone();
            let mut vars = vec![];
            self.free_tvars(&fty, &mut vars);
            *g.scheme.borrow_mut() = Scheme { vars, ty: fty };
            self.globals.insert(fd.name.clone(), g);
        }
        Ok(())
    }

    // --- 運算式
    fn coerce(&mut self, actual: &Type, expected: &Type, line: usize) -> R<()> {
        let (a, x) = (self.prune(actual), self.prune(expected));
        if let (Type::Ref(ai, am), Type::Ref(xi, xm)) = (&a, &x) {
            if *am && !*xm { return self.unify(ai, xi, line); }
        }
        if let (Type::Ref(ai, _), Type::Fn(..)) = (&a, &x) {
            if matches!(self.prune(ai), Type::Fn(..)) { return self.unify(ai, &x, line); }
        }
        self.unify(&a, &x, line)
    }

    fn infer(&mut self, e: &mut Expr) -> R<Type> {
        let line = e.line;
        let t = match &mut e.kind {
            EK::Int(_) => t_int(),
            EK::Float(_) => t_float(),
            EK::Str(_) => t_string(),
            EK::Bool(_) => t_bool(),
            EK::Unit => t_unit(),
            EK::Var { name, var } => {
                let v = self.lookup(name, line)?;
                let t = match &v {
                    VarRef::Local(x) => x.ty.clone(),
                    VarRef::Global(g) => { let s = g.scheme.borrow().clone(); self.inst(&s) }
                    VarRef::Builtin(_) => return err(format!("內建函式 `{}` 只能直接呼叫，不能當作值", name), line),
                };
                *var = Some(v);
                t
            }
            EK::Ann(inner, ast) => {
                let t = self.infer(inner)?;
                let c = self.conv_fn(ast, line)?;
                self.unify(&t, &c, line)?;
                t
            }
            EK::Tuple(items) => {
                let mut v = vec![];
                for x in items.iter_mut() { v.push(self.infer(x)?); }
                Type::Tuple(v)
            }
            EK::List(items) => {
                let el = self.fresh_t();
                for x in items.iter_mut() { let t = self.infer(x)?; self.unify(&t, &el, x.line)?; }
                Type::Con("list".into(), vec![el])
            }
            EK::Ctor { name, arg, args, info } => {
                let ci = match self.ctors.get(name.as_str()) {
                    Some(c) => c.clone(),
                    None => return err(format!("未定義的建構子 `{}`", name), line),
                };
                let (fts, res) = self.ctor_inst(&ci);
                let arity = fts.len();
                let v: Vec<Expr> = match arg.take() {
                    None => vec![],
                    Some(a) => {
                        let Expr { line: l, ty, kind } = *a;
                        match kind {
                            EK::Tuple(items) if arity >= 2 => items,
                            k => vec![Expr { line: l, ty, kind: k }],
                        }
                    }
                };
                if v.len() != arity {
                    return err(format!("建構子 `{}` 需要 {} 個引數，但給了 {} 個", name, arity, v.len()), line);
                }
                *args = v;
                for (a, ft) in args.iter_mut().zip(fts.iter()) {
                    let at = self.infer(a)?;
                    self.coerce(&at, ft, a.line)?;
                }
                *info = Some(ci);
                res
            }
            EK::App { f, args, builtin } => {
                let fl = f.line;
                if let EK::Var { name, var } = &mut f.kind {
                    if let VarRef::Builtin(b) = self.lookup(name, fl)? {
                        let ft = { let s = b.scheme.clone(); self.inst(&s) };
                        let (ps, ret) = match ft { Type::Fn(ps, r) => (ps, *r), _ => unreachable!() };
                        if args.len() != ps.len() {
                            return err(format!("`{}` 需要 {} 個引數，但給了 {} 個", b.name, ps.len(), args.len()), line);
                        }
                        for (a, p) in args.iter_mut().zip(ps.iter()) {
                            let at = self.infer(a)?;
                            self.coerce(&at, p, a.line)?;
                            if b.name == "clone" { self.reqs.push((at, "Clone")); }
                        }
                        *var = Some(VarRef::Builtin(b.clone()));
                        *builtin = Some(b);
                        e.ty = ret.clone();
                        return Ok(ret);
                    }
                }
                let ft0 = self.infer(f)?;
                let ft = self.prune(&ft0);
                let mut ats = vec![];
                for a in args.iter_mut() { ats.push(self.infer(a)?); }
                match ft {
                    Type::Fn(ps, ret) => {
                        if ps.len() != ats.len() {
                            return err(format!("函式需要 {} 個引數，但給了 {} 個（不支援部分套用）", ps.len(), ats.len()), line);
                        }
                        for ((a, at), p) in args.iter().zip(ats.iter()).zip(ps.iter()) { self.coerce(at, p, a.line)?; }
                        *ret
                    }
                    Type::Var(_) => {
                        let r = self.fresh_t();
                        self.unify(&ft, &Type::Fn(ats, Box::new(r.clone())), line)?;
                        r
                    }
                    other => return err(format!("型別 {} 不是函式，不能呼叫", self.show(&other)), line),
                }
            }
            EK::Bin(op, l, r) => {
                let lt = self.infer(l)?;
                let rt = self.infer(r)?;
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "mod" => {
                        self.unify(&lt, &t_int(), l.line)?; self.unify(&rt, &t_int(), r.line)?; t_int()
                    }
                    "+." | "-." | "*." | "/." => {
                        self.unify(&lt, &t_float(), l.line)?; self.unify(&rt, &t_float(), r.line)?; t_float()
                    }
                    "=" | "<>" | "<" | ">" | "<=" | ">=" => {
                        self.unify(&rt, &lt, r.line)?;
                        self.reqs.push((lt.clone(), "PartialEq"));
                        self.reqs.push((lt, "PartialOrd"));
                        t_bool()
                    }
                    "&&" | "||" => {
                        self.unify(&lt, &t_bool(), l.line)?; self.unify(&rt, &t_bool(), r.line)?; t_bool()
                    }
                    "^" => {
                        self.unify(&lt, &t_string(), l.line)?; self.unify(&rt, &t_string(), r.line)?; t_string()
                    }
                    _ => return err(format!("未知運算子 {}", op), line),
                }
            }
            EK::Un(op, x) => {
                let t = self.infer(x)?;
                let want = match op.as_str() { "neg" => t_int(), "fneg" => t_float(), _ => t_bool() };
                self.unify(&t, &want, line)?;
                want
            }
            EK::If(c, a, b) => {
                let ct = self.infer(c)?;
                self.unify(&ct, &t_bool(), c.line)?;
                let at = self.infer(a)?;
                match b {
                    None => { self.unify(&at, &t_unit(), a.line)?; t_unit() }
                    Some(b) => { let bt = self.infer(b)?; self.unify(&bt, &at, b.line)?; at }
                }
            }
            EK::Seq(a, b) => {
                let t = self.infer(a)?;
                if self.unify(&t, &t_unit(), a.line).is_err() {
                    return err(format!("`;` 左邊必須是 unit，實際是 {}（請用 let 綁定它）", self.show(&t)), a.line);
                }
                self.infer(b)?
            }
            EK::Let { pat, val, body } => {
                let vt = self.infer(val)?;
                self.scopes.push(HashMap::new());
                self.infer_pat(pat, &vt, Bm::N)?;
                let t = self.infer(body)?;
                self.scopes.pop();
                t
            }
            EK::Match(scrut, arms) => {
                let st = self.infer(scrut)?;
                let res = self.fresh_t();
                for (p, body) in arms.iter_mut() {
                    self.scopes.push(HashMap::new());
                    self.infer_pat(p, &st, Bm::N)?;
                    let bt = self.infer(body)?;
                    self.unify(&bt, &res, body.line)?;
                    self.scopes.pop();
                }
                res
            }
            EK::Fun { params, body, pinfos } => {
                self.scopes.push(HashMap::new());
                pinfos.clear();
                let mut pts = vec![];
                for p in params.iter() {
                    let t = match &p.ty { Some(a) => self.conv_fn(a, p.line)?, None => self.fresh_t() };
                    pts.push(t.clone());
                    pinfos.push(self.bind(&p.name, t, p.mutable, Boxed::No));
                }
                let bt = self.infer(body)?;
                self.scopes.pop();
                Type::Fn(pts, Box::new(bt))
            }
            EK::Ref(x, m) => {
                let t = self.infer(x)?;
                if *m {
                    if let EK::Var { name, var: Some(VarRef::Local(v)) } = &x.kind {
                        if !v.mutable { return err(format!("不能可變借用 `{}`：它不是 `let mut`", name), line); }
                    }
                }
                Type::Ref(Box::new(t), *m)
            }
            EK::Deref(x) => {
                let t0 = self.infer(x)?;
                match self.prune(&t0) {
                    Type::Ref(i, _) => *i,
                    Type::Var(v) => {
                        let i = self.fresh_t();
                        self.unify(&Type::Var(v), &Type::Ref(Box::new(i.clone()), false), line)?;
                        i
                    }
                    other => return err(format!("只能對參考解參考 (!)，但型別是 {}", self.show(&other)), line),
                }
            }
            EK::Assign(l, r) => {
                let lt0 = self.infer(l)?;
                let lt = self.prune(&lt0);
                let rt = self.infer(r)?;
                match lt {
                    Type::Ref(i, true) => { self.unify(&rt, &i, r.line)?; t_unit() }
                    other => return err(format!("`:=` 左邊必須是 &mut 參考，但型別是 {}", self.show(&other)), line),
                }
            }
            EK::SetVar { name, e: rhs, var } => {
                let v = match self.lookup(name, line)? {
                    VarRef::Local(v) => v,
                    _ => return err(format!("`{}` 不是區域變數", name), line),
                };
                if !v.mutable { return err(format!("不能重新賦值 `{}`：它不是 `let mut`", name), line); }
                let rt = self.infer(rhs)?;
                self.unify(&rt, &v.ty, line)?;
                *var = Some(v);
                t_unit()
            }
        };
        e.ty = t.clone();
        Ok(t)
    }

    // --- 模式
    fn strip(&self, ty: &Type, mode: Bm) -> (Type, Bm) {
        let (mut t, mut mode) = (self.prune(ty), mode);
        while let Type::Ref(i, m) = &t {
            mode = if *m && matches!(mode, Bm::N | Bm::M) { Bm::M } else { Bm::R };
            let n = self.prune(i);
            t = n;
        }
        (t, mode)
    }

    fn infer_pat(&mut self, p: &mut Pat, ty: &Type, mode: Bm) -> R<()> {
        let line = p.line;
        match &mut p.kind {
            PK::Wild => Ok(()),
            PK::Var { name, mutable, var } => {
                if mode != Bm::N && *mutable { return err("參考模式下的綁定不能是 `mut`", line); }
                *var = Some(self.bind(name, wrap(ty.clone(), mode), *mutable, Boxed::No));
                Ok(())
            }
            PK::Lit { int, .. } => {
                let (t, _) = self.strip(ty, mode);
                self.unify(&t, &if *int { t_int() } else { t_bool() }, line)
            }
            PK::Unit => { let (t, _) = self.strip(ty, mode); self.unify(&t, &t_unit(), line) }
            PK::Tuple(items) => {
                let (t, m) = self.strip(ty, mode);
                let tts: Vec<Type> = items.iter().map(|_| self.fresh_t()).collect();
                self.unify(&t, &Type::Tuple(tts.clone()), line)?;
                for (sp, tt) in items.iter_mut().zip(tts.iter()) { self.infer_pat(sp, tt, m)?; }
                Ok(())
            }
            PK::Ctor { name, arg, args, info } => {
                let ci = match self.ctors.get(name.as_str()) {
                    Some(c) => c.clone(),
                    None => return err(format!("未定義的建構子 `{}`", name), line),
                };
                let (t, m) = self.strip(ty, mode);
                let (fts, res) = self.ctor_inst(&ci);
                self.unify(&t, &res, line)?;
                if args.is_none() {
                    let v: Vec<Pat> = match arg.take() {
                        None => vec![],
                        Some(b) => {
                            let Pat { line: l, kind } = *b;
                            match kind {
                                PK::Tuple(items) if fts.len() >= 2 => items,
                                k => vec![Pat { line: l, kind: k }],
                            }
                        }
                    };
                    if v.len() != fts.len() {
                        return err(format!("建構子 `{}` 需要 {} 個引數，但給了 {} 個", name, fts.len(), v.len()), line);
                    }
                    *args = Some(v);
                } else if args.as_ref().unwrap().len() != fts.len() {
                    return err(format!("`{}` 模式引數數量錯誤", name), line);
                }
                for (i, (sp, ft)) in args.as_mut().unwrap().iter_mut().zip(fts.iter()).enumerate() {
                    let sl = sp.line;
                    if ci.boxed[i] {
                        match &mut sp.kind {
                            PK::Wild => continue,
                            PK::Var { name: n, mutable: mu, var: v } => {
                                if m != Bm::N && *mu { return err("參考模式下的綁定不能是 `mut`", sl); }
                                let bx = match m { Bm::N => Boxed::Own, Bm::R => Boxed::Ref, Bm::M => Boxed::Mut };
                                *v = Some(self.bind(n, wrap(ft.clone(), m), *mu, bx));
                            }
                            _ => return err("遞迴欄位的子模式只能是變數或 `_`；請先綁定變數再用巢狀 match", sl),
                        }
                    } else {
                        self.infer_pat(sp, ft, m)?;
                    }
                }
                *info = Some(ci);
                Ok(())
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 所有權檢查 (move checker)
// ════════════════════════════════════════════════════════════════
fn pat_vars(p: &Pat, acc: &mut Vec<Rc<VarInfo>>) {
    match &p.kind {
        PK::Var { var: Some(v), .. } => acc.push(v.clone()),
        PK::Tuple(xs) => for x in xs { pat_vars(x, acc) },
        PK::Ctor { args: Some(xs), .. } => for x in xs { pat_vars(x, acc) },
        _ => {}
    }
}

fn subexprs(e: &Expr) -> Vec<&Expr> {
    match &e.kind {
        EK::Tuple(xs) | EK::List(xs) => xs.iter().collect(),
        EK::Ctor { args, .. } => args.iter().collect(),
        EK::App { f, args, .. } => { let mut v: Vec<&Expr> = vec![f]; v.extend(args.iter()); v }
        EK::Bin(_, l, r) => vec![l, r],
        EK::Un(_, x) | EK::Ref(x, _) | EK::Deref(x) | EK::Ann(x, _) => vec![x],
        EK::SetVar { e, .. } => vec![e],
        EK::If(c, a, b) => { let mut v: Vec<&Expr> = vec![c, a]; if let Some(b) = b { v.push(b); } v }
        EK::Match(s, arms) => { let mut v: Vec<&Expr> = vec![s]; v.extend(arms.iter().map(|(_, b)| b)); v }
        EK::Let { val, body, .. } => vec![val, body],
        EK::Fun { body, .. } => vec![body],
        EK::Seq(a, b) | EK::Assign(a, b) => vec![a, b],
        _ => vec![],
    }
}

fn collect_vars(e: &Expr, refs: &mut HashMap<usize, Rc<VarInfo>>, defs: &mut HashSet<usize>) {
    match &e.kind {
        EK::Var { var: Some(VarRef::Local(v)), .. } => { refs.insert(v.id, v.clone()); }
        EK::SetVar { var: Some(v), .. } => { refs.insert(v.id, v.clone()); }
        EK::Let { pat, .. } => { let mut vs = vec![]; pat_vars(pat, &mut vs); defs.extend(vs.iter().map(|v| v.id)); }
        EK::Match(_, arms) => for (p, _) in arms {
            let mut vs = vec![]; pat_vars(p, &mut vs); defs.extend(vs.iter().map(|v| v.id));
        },
        EK::Fun { pinfos, .. } => defs.extend(pinfos.iter().map(|v| v.id)),
        _ => {}
    }
    for c in subexprs(e) { collect_vars(c, refs, defs); }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode { Move, Read }

struct Checker<'a> { inf: &'a Infer, moved: HashMap<usize, usize> }

impl<'a> Checker<'a> {
    fn is_copy(&self, t: &Type) -> bool {
        match self.inf.prune(t) {
            Type::Var(_) => false,
            Type::Tuple(xs) => xs.iter().all(|x| self.is_copy(x)),
            Type::Ref(..) | Type::Fn(..) => true,   // &mut 以 reborrow 處理；閉包是否 Copy 由 rustc 驗證
            Type::Con(n, args) => match n.as_str() {
                "int" | "float" | "bool" | "unit" => true,
                "string" => false,
                _ => self.inf.types[&n].copy_ok && args.iter().all(|a| self.is_copy(a)),
            },
        }
    }

    fn use_var(&mut self, v: &VarInfo, mode: Mode, line: usize) -> R<()> {
        if let Some(l) = self.moved.get(&v.id) {
            return err(format!("使用了已被移動的值 `{}`（在第 {} 行被移動；可以改用借用 `&{}`，或 `clone`）",
                               v.name, l, v.name), line);
        }
        if mode == Mode::Move && !self.is_copy(&v.ty) { self.moved.insert(v.id, line); }
        Ok(())
    }

    fn check_fn(&mut self, fd: &FunDef) -> R<()> {
        self.moved.clear();
        self.walk(&fd.body, Mode::Move)
    }

    fn walk(&mut self, e: &Expr, mode: Mode) -> R<()> {
        match &e.kind {
            EK::Var { var: Some(VarRef::Local(v)), .. } => self.use_var(v, mode, e.line)?,
            EK::Var { .. } | EK::Int(_) | EK::Float(_) | EK::Str(_) | EK::Bool(_) | EK::Unit => {}
            EK::Tuple(_) | EK::List(_) | EK::Ctor { .. } => for x in subexprs(e) { self.walk(x, Mode::Move)?; },
            EK::Ann(x, _) => self.walk(x, mode)?,
            EK::Ref(x, _) => self.walk(x, Mode::Read)?,
            EK::Deref(x) => {
                if mode == Mode::Move && !self.is_copy(&e.ty) {
                    return err("不能從參考中移出非 Copy 的值（請用 `clone`）", e.line);
                }
                self.walk(x, Mode::Read)?;
            }
            EK::App { f, args, builtin } => {
                if builtin.is_none() {
                    self.walk(f, if matches!(f.kind, EK::Var { .. }) { Mode::Read } else { Mode::Move })?;
                }
                for (i, a) in args.iter().enumerate() {
                    let borrowed = builtin.as_ref().map_or(false, |b| b.borrow[i]);
                    self.walk(a, if borrowed { Mode::Read } else { Mode::Move })?;
                }
            }
            EK::Bin(op, l, r) => {
                let (ml, mr) = match op.as_str() {
                    "=" | "<>" | "<" | ">" | "<=" | ">=" => (Mode::Read, Mode::Read),
                    "^" => (Mode::Move, Mode::Read),
                    _ => (Mode::Move, Mode::Move),
                };
                self.walk(l, ml)?; self.walk(r, mr)?;
            }
            EK::Un(_, x) => self.walk(x, Mode::Move)?,
            EK::Seq(a, b) => { self.walk(a, Mode::Move)?; self.walk(b, mode)?; }
            EK::Let { val, body, .. } => { self.walk(val, Mode::Move)?; self.walk(body, mode)?; }
            EK::Assign(l, r) => { self.walk(l, Mode::Read)?; self.walk(r, Mode::Move)?; }
            EK::SetVar { e: rhs, var, .. } => {
                self.walk(rhs, Mode::Move)?;
                if let Some(v) = var { self.moved.remove(&v.id); }
            }
            EK::If(c, a, b) => {
                self.walk(c, Mode::Move)?;
                let start = self.moved.clone();
                self.walk(a, mode)?;
                let m1 = std::mem::replace(&mut self.moved, start);
                if let Some(b) = b { self.walk(b, mode)?; }
                for (k, v) in m1 { self.moved.entry(k).or_insert(v); }
            }
            EK::Match(s, arms) => {
                let mut moves = false;
                for (p, _) in arms {
                    let mut vs = vec![]; pat_vars(p, &mut vs);
                    if vs.iter().any(|v| !self.is_copy(&v.ty)) { moves = true; }
                }
                self.walk(s, if moves { Mode::Move } else { Mode::Read })?;
                let start = self.moved.clone();
                let mut merged = HashMap::new();
                for (_, body) in arms {
                    self.moved = start.clone();
                    self.walk(body, mode)?;
                    for (k, v) in self.moved.drain() { merged.insert(k, v); }
                }
                self.moved = merged;
            }
            EK::Fun { body, .. } => {
                let (mut refs, mut defs) = (HashMap::new(), HashSet::new());
                collect_vars(e, &mut refs, &mut defs);
                let mut caps: Vec<_> = refs.into_iter().filter(|(id, _)| !defs.contains(id)).collect();
                caps.sort_by_key(|(id, _)| *id);
                for (_, v) in caps { self.use_var(&v, Mode::Move, e.line)?; }   // move 閉包捕獲
                let saved = std::mem::take(&mut self.moved);
                self.walk(body, Mode::Move)?;
                self.moved = saved;
            }
        }
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════
// Rust 程式碼產生
// ════════════════════════════════════════════════════════════════
const RUST_KW: &[&str] = &["as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
    "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "async",
    "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof",
    "unsized", "virtual", "yield", "try"];

const RUNTIME: &str = r##"#![allow(warnings)]
// ── RML runtime：極薄的內建函式，全部會被 inline ──
#[inline(always)] fn rml_print_int(x: i64) { print!("{}", x); }
#[inline(always)] fn rml_print_float(x: f64) { print!("{:?}", x); }
#[inline(always)] fn rml_print_string(s: &String) { print!("{}", s); }
#[inline(always)] fn rml_print_endline(s: &String) { println!("{}", s); }
#[inline(always)] fn rml_print_newline(_u: ()) { println!(); }
#[inline(always)] fn rml_string_of_int(x: i64) -> String { x.to_string() }
#[inline(always)] fn rml_string_of_float(x: f64) -> String { format!("{:?}", x) }
#[inline(always)] fn rml_string_length(s: &String) -> i64 { s.len() as i64 }
#[inline(always)] fn rml_concat(mut a: String, b: &String) -> String { a.push_str(b); a }
#[inline(always)] fn rml_float_of_int(x: i64) -> f64 { x as f64 }
#[inline(always)] fn rml_int_of_float(x: f64) -> i64 { x as i64 }
#[inline(always)] fn rml_sqrt(x: f64) -> f64 { x.sqrt() }
#[inline(always)] fn rml_abs(x: i64) -> i64 { x.abs() }
#[inline(always)] fn rml_min(a: i64, b: i64) -> i64 { if a < b { a } else { b } }
#[inline(always)] fn rml_max(a: i64, b: i64) -> i64 { if a > b { a } else { b } }
#[inline(always)] fn rml_clone<T: Clone>(x: &T) -> T { x.clone() }
"##;

const MAIN_WRAPPER: &str = r##"fn main() {
    // 以大堆疊執行緒跑使用者程式，深遞迴 / 長鏈結串列的遞迴 drop 才不會爆堆疊
    let h = std::thread::Builder::new().stack_size(1 << 30).spawn(|| rml_user_main(())).unwrap();
    h.join().unwrap();
}"##;

fn rn(name: &str) -> String {
    let n = name.replace('\'', "_p");
    if RUST_KW.contains(&n.as_str()) { n + "_" } else { n }
}

fn rust_str(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c if (c as u32) < 32 => o.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

fn rust_op(op: &str) -> &str {
    match op { "mod" => "%", "+." => "+", "-." => "-", "*." => "*", "/." => "/", "<>" => "!=", "=" => "==", o => o }
}

struct Gen<'a> {
    inf: &'a Infer,
    tvn: HashMap<usize, String>,
    lt: bool,
    cur: Option<String>,
    uses_loop: bool,
    pnames: Vec<String>,
}

impl<'a> Gen<'a> {
    fn new(inf: &'a Infer) -> Self {
        Gen { inf, tvn: HashMap::new(), lt: false, cur: None, uses_loop: false, pnames: vec![] }
    }

    fn ty(&self, t: &Type, copy_fn: bool) -> String {
        match self.inf.prune(t) {
            Type::Var(i) => self.tvn.get(&i).cloned().unwrap_or_else(|| "_".into()),
            Type::Tuple(xs) => {
                let v: Vec<String> = xs.iter().map(|x| self.ty(x, false)).collect();
                if v.len() == 1 { format!("({},)", v[0]) } else { format!("({})", v.join(", ")) }
            }
            Type::Ref(i, m) => format!("&{}{}{}", if self.lt { "'a " } else { "" },
                                       if m { "mut " } else { "" }, self.ty(&i, false)),
            Type::Fn(ps, r) => {
                let r = self.inf.prune(&r);
                let ret = if is_con(&r, "unit") { String::new() } else { format!(" -> {}", self.ty(&r, false)) };
                let ps: Vec<String> = ps.iter().map(|x| self.ty(x, false)).collect();
                format!("impl Fn({}){}{}", ps.join(", "), ret, if copy_fn { " + Copy" } else { "" })
            }
            Type::Con(n, args) => match n.as_str() {
                "int" => "i64".into(), "float" => "f64".into(), "bool" => "bool".into(),
                "unit" => "()".into(), "string" => "String".into(),
                _ => {
                    let rn_ = &self.inf.types[&n].rname;
                    if args.is_empty() { rn_.clone() }
                    else { format!("{}<{}>", rn_, args.iter().map(|a| self.ty(a, false)).collect::<Vec<_>>().join(", ")) }
                }
            },
        }
    }

    fn has_fn(&self, t: &Type) -> bool {
        match self.inf.prune(t) {
            Type::Fn(..) => true,
            Type::Con(_, a) | Type::Tuple(a) => a.iter().any(|x| self.has_fn(x)),
            Type::Ref(i, _) => self.has_fn(&i),
            Type::Var(_) => false,
        }
    }

    fn closure_ty(&self, t: &Type) -> Option<String> {
        if self.has_fn(t) { None } else { Some(self.ty(t, false)) }
    }

    fn typedecl(&mut self, ti: &TypeInfo) -> String {
        self.tvn = ti.pvars.iter().zip(ti.params.iter()).map(|(v, p)| (*v, format!("T{}", p))).collect();
        self.lt = false;
        let gen = if ti.pvars.is_empty() { String::new() }
                  else { format!("<{}>", ti.pvars.iter().map(|v| self.tvn[v].clone()).collect::<Vec<_>>().join(", ")) };
        let derive = format!("Debug, Clone, PartialEq, PartialOrd{}", if ti.copy_ok { ", Copy" } else { "" });
        let mut out = vec![format!("#[derive({})]", derive), format!("enum {}{} {{", ti.rname, gen)];
        for c in &ti.ctors {
            if c.ftypes.is_empty() { out.push(format!("    {},", c.name)); }
            else {
                let fs: Vec<String> = c.ftypes.iter().zip(c.boxed.iter())
                    .map(|(t, b)| if *b { format!("Box<{}>", self.ty(t, false)) } else { self.ty(t, false) }).collect();
                out.push(format!("    {}({}),", c.name, fs.join(", ")));
            }
        }
        out.push("}".into());
        out.join("\n")
    }

    fn fundef(&mut self, fd: &FunDef) -> String {
        let g = fd.gfn.as_ref().unwrap();
        let gt = self.inf.prune(&g.scheme.borrow().ty);
        let (gparams, gret) = match gt.clone() { Type::Fn(p, r) => (p, *r), _ => unreachable!() };
        let mut tvs = vec![];
        self.inf.free_tvars(&gt, &mut tvs);
        self.tvn = tvs.iter().enumerate().map(|(i, v)| (*v, format!("T{}", i))).collect();
        self.lt = self.inf.contains_ref(&gret);
        let mut bounds: HashMap<usize, HashSet<&str>> = HashMap::new();
        for (t, b) in &fd.reqs {
            let mut vs = vec![];
            self.inf.free_tvars(t, &mut vs);
            for v in vs { if self.tvn.contains_key(&v) { bounds.entry(v).or_default().insert(*b); } }
        }
        let mut gens: Vec<String> = if self.lt { vec!["'a".into()] } else { vec![] };
        for v in &tvs {
            let bs: Vec<&str> = ["PartialEq", "PartialOrd", "Clone"].iter().copied()
                .filter(|b| bounds.get(v).map_or(false, |s| s.contains(b))).collect();
            gens.push(if bs.is_empty() { self.tvn[v].clone() } else { format!("{}: {}", self.tvn[v], bs.join(" + ")) });
        }
        self.cur = Some(fd.name.clone());
        self.uses_loop = false;
        self.pnames = fd.pinfos.iter().enumerate()
            .map(|(i, vi)| if vi.name == "_unit" { format!("_u{}", i) } else { rn(&vi.name) }).collect();
        let mut body = self.g(&fd.body, true);
        if self.uses_loop { body = format!("loop {{ return {}; }}", body); }
        let ps: Vec<String> = fd.pinfos.iter().zip(gparams.iter()).enumerate().map(|(i, (vi, pt))| {
            format!("{}{}: {}", if vi.mutable || self.uses_loop { "mut " } else { "" }, self.pnames[i], self.ty(pt, true))
        }).collect();
        let r = self.inf.prune(&gret);
        let ret = if is_con(&r, "unit") { String::new() } else { format!(" -> {}", self.ty(&gret, true)) };
        let name = if fd.name == "main" { "rml_user_main".to_string() } else { rn(&fd.name) };
        let gs = if gens.is_empty() { String::new() } else { format!("<{}>", gens.join(", ")) };
        self.cur = None;
        format!("fn {}{}({}){} {{\n    {}\n}}", name, gs, ps.join(", "), ret, body)
    }

    // --- 運算式
    fn g(&mut self, e: &Expr, tail: bool) -> String {
        match &e.kind {
            EK::Int(v) => format!("{}i64", v),
            EK::Float(v) => format!("{:?}f64", v.parse::<f64>().unwrap_or(0.0)),
            EK::Str(v) => format!("String::from(\"{}\")", rust_str(v)),
            EK::Bool(b) => b.to_string(),
            EK::Unit => "()".into(),
            EK::Ann(x, _) => self.g(x, tail),
            EK::Tuple(xs) => {
                let v: Vec<String> = xs.iter().map(|x| self.g(x, false)).collect();
                format!("({})", v.join(", "))
            }
            EK::Var { var, name } => match var {
                Some(VarRef::Global(g)) => if g.name == "main" { "rml_user_main".into() } else { rn(&g.name) },
                Some(VarRef::Local(v)) => {
                    let n = rn(&v.name);
                    match v.boxed {
                        Boxed::Own => format!("(*{})", n),
                        Boxed::Ref => format!("(&**{})", n),
                        Boxed::Mut => format!("(&mut **{})", n),
                        Boxed::No => n,
                    }
                }
                _ => rn(name),
            },
            EK::Ctor { info, args, .. } => self.build_ctor(info.as_ref().unwrap(), args),
            EK::List(items) => {
                let nil = self.inf.ctors["Nil"].clone();
                let cons = self.inf.ctors["Cons"].clone();
                let mut out = self.build_ctor(&nil, &[]);
                for x in items.iter().rev() {
                    out = format!("{}::Cons({}, Box::new({}))", self.inf.types[&cons.tname].rname, self.g(x, false), out);
                }
                out
            }
            EK::App { f, args, builtin } => {
                let a: Vec<String> = args.iter().map(|x| self.g(x, false)).collect();
                if let Some(b) = builtin {
                    let a2: Vec<String> = a.iter().zip(b.borrow.iter())
                        .map(|(s, bf)| if *bf { format!("&({})", s) } else { s.clone() }).collect();
                    return format!("rml_{}({})", b.name, a2.join(", "));
                }
                if tail {
                    if let EK::Var { var: Some(VarRef::Global(g)), .. } = &f.kind {
                        if Some(&g.name) == self.cur.as_ref() && a.len() == self.pnames.len() {
                            self.uses_loop = true;
                            let mut parts: Vec<String> = a.iter().enumerate().map(|(i, x)| format!("let __t{} = {};", i, x)).collect();
                            parts.extend(self.pnames.iter().enumerate().map(|(i, p)| format!("{} = __t{};", p, i)));
                            return format!("{{ {} continue; }}", parts.join(" "));
                        }
                    }
                }
                let fs = self.g(f, false);
                let fs = if matches!(f.kind, EK::Var { .. }) { fs } else { format!("({})", fs) };
                format!("{}({})", fs, a.join(", "))
            }
            EK::Bin(op, l, r) => {
                let (l, r) = (self.g(l, false), self.g(r, false));
                if op == "^" { format!("rml_concat({}, &({}))", l, r) }
                else { format!("({} {} {})", l, rust_op(op), r) }
            }
            EK::Un(op, x) => {
                let x = self.g(x, false);
                if op == "not" { format!("(!{})", x) } else { format!("(-{})", x) }
            }
            EK::If(c, a, b) => {
                let c = self.g(c, false);
                match b {
                    None => format!("if {} {{ {} }}", c, self.g(a, false)),
                    Some(b) => { let a = self.g(a, tail); let b = self.g(b, tail); format!("if {} {{ {} }} else {{ {} }}", c, a, b) }
                }
            }
            EK::Seq(a, b) => { let a = self.g(a, false); let b = self.g(b, tail); format!("{{ {}; {} }}", a, b) }
            EK::Let { pat, val, body } => {
                let v = self.g(val, false);
                let b = self.g(body, tail);
                format!("{{ let {} = {}; {} }}", self.pat(pat), v, b)
            }
            EK::Match(s, arms) => {
                let sc = self.g(s, false);
                let mut v = vec![];
                for (p, b) in arms { let bs = self.g(b, tail); v.push(format!("{} => {{ {} }}", self.pat(p), bs)); }
                format!("match {} {{\n        {}\n    }}", sc, v.join(",\n        "))
            }
            EK::Fun { params, body, pinfos } => {
                let mut ps = vec![];
                for (vi, _) in pinfos.iter().zip(params.iter()) {
                    let mut s = format!("{}{}", if vi.mutable { "mut " } else { "" }, rn(&vi.name));
                    if let Some(t) = self.closure_ty(&vi.ty) { if t != "_" { s.push_str(&format!(": {}", t)); } }
                    ps.push(s);
                }
                let saved = (self.cur.take(), self.uses_loop, std::mem::take(&mut self.pnames));
                let b = self.g(body, false);
                self.cur = saved.0; self.uses_loop = saved.1; self.pnames = saved.2;
                let ret = self.closure_ty(&body.ty).filter(|r| !r.contains('_'));
                match ret {
                    Some(r) => format!("move |{}| -> {} {{ {} }}", ps.join(", "), r, b),
                    None => format!("move |{}| {{ {} }}", ps.join(", "), b),
                }
            }
            EK::Ref(x, m) => format!("(&{}{})", if *m { "mut " } else { "" }, self.g(x, false)),
            EK::Deref(x) => format!("(*{})", self.g(x, false)),
            EK::Assign(l, r) => { let l = self.g(l, false); let r = self.g(r, false); format!("{{ *{} = {}; }}", l, r) }
            EK::SetVar { name, e: rhs, .. } => format!("{{ {} = {}; }}", rn(name), self.g(rhs, false)),
        }
    }

    fn build_ctor(&mut self, info: &CtorInfo, args: &[Expr]) -> String {
        let pre = format!("{}::{}", self.inf.types[&info.tname].rname, info.name);
        if args.is_empty() { return pre; }
        let mut out = vec![];
        for (a, b) in args.iter().zip(info.boxed.iter()) {
            if *b {
                if let EK::Var { var: Some(VarRef::Local(v)), .. } = &a.kind {
                    if v.boxed == Boxed::Own { out.push(rn(&v.name)); continue; }   // 已是 Box，直接重用
                }
                out.push(format!("Box::new({})", self.g(a, false)));
            } else { out.push(self.g(a, false)); }
        }
        format!("{}({})", pre, out.join(", "))
    }

    // --- 模式
    fn pat(&self, p: &Pat) -> String {
        match &p.kind {
            PK::Wild => "_".into(),
            PK::Var { name, mutable, .. } => format!("{}{}", if *mutable { "mut " } else { "" }, rn(name)),
            PK::Lit { v, .. } => v.clone(),
            PK::Unit => "()".into(),
            PK::Tuple(xs) => format!("({})", xs.iter().map(|x| self.pat(x)).collect::<Vec<_>>().join(", ")),
            PK::Ctor { info, args, .. } => {
                let info = info.as_ref().unwrap();
                let pre = format!("{}::{}", self.inf.types[&info.tname].rname, info.name);
                match args {
                    Some(a) if !a.is_empty() =>
                        format!("{}({})", pre, a.iter().map(|x| self.pat(x)).collect::<Vec<_>>().join(", ")),
                    _ => pre,
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 主流程
// ════════════════════════════════════════════════════════════════
fn compile_source(src: &str) -> R<String> {
    let pre = Parser::new(lex(PRELUDE)?).parse_program()?;
    let user = Parser::new(lex(src)?).parse_program()?;
    let mut inf = Infer::new();
    let mut tnames: Vec<String> = vec![];
    let mut fdefs: Vec<FunDef> = vec![];
    for d in pre.into_iter().chain(user.into_iter()) {
        match d {
            Decl::Types(defs) => {
                inf.declare_types(&defs)?;
                tnames.extend(defs.iter().map(|d| d.name.clone()));
            }
            Decl::Funs(mut fds) => {
                inf.infer_group(&mut fds)?;
                fdefs.extend(fds);
            }
        }
    }
    let main_ok = match inf.globals.get("main") {
        None => return err("缺少進入點：請定義 `let main () = ...`", 0),
        Some(g) => {
            let t = inf.prune(&g.scheme.borrow().ty);
            matches!(&t, Type::Fn(ps, _) if ps.len() == 1 && is_con(&inf.prune(&ps[0]), "unit"))
        }
    };
    if !main_ok { return err("`main` 必須是 `let main () = ...`", 0); }
    {
        let mut chk = Checker { inf: &inf, moved: HashMap::new() };
        for fd in &fdefs { chk.check_fn(fd)?; }
    }
    let mut g = Gen::new(&inf);
    let mut out = vec![RUNTIME.to_string()];
    for n in &tnames { out.push(g.typedecl(&inf.types[n])); }
    for fd in &fdefs { out.push(g.fundef(fd)); }
    out.push(MAIN_WRAPPER.to_string());
    Ok(out.join("\n\n") + "\n")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut src, mut output, mut run, mut to_stdout) = (None, None, false, false);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => { i += 1; output = args.get(i).cloned(); }
            "--run" => run = true,
            "--stdout" => to_stdout = true,
            "-h" | "--help" => {
                println!("用法: rmlc <prog.rml> [-o out.rs] [--run] [--stdout]");
                return;
            }
            a => src = Some(a.to_string()),
        }
        i += 1;
    }
    let src = match src {
        Some(s) => s,
        None => { eprintln!("用法: rmlc <prog.rml> [-o out.rs] [--run] [--stdout]"); std::process::exit(2); }
    };
    let text = std::fs::read_to_string(&src).unwrap_or_else(|e| {
        eprintln!("無法讀取 {}: {}", src, e); std::process::exit(1)
    });
    let rust = match compile_source(&text) {
        Ok(r) => r,
        Err(e) => {
            if e.line > 0 { eprintln!("{}:{}: 錯誤: {}", src, e.line, e.msg); }
            else { eprintln!("{}: 錯誤: {}", src, e.msg); }
            std::process::exit(1);
        }
    };
    if to_stdout { print!("{}", rust); return; }
    let out = output.unwrap_or_else(|| {
        let p = std::path::Path::new(&src);
        p.with_extension("rs").to_string_lossy().into_owned()
    });
    std::fs::write(&out, &rust).unwrap_or_else(|e| { eprintln!("無法寫入 {}: {}", out, e); std::process::exit(1) });
    println!("已產生 {}", out);
    if run {
        let exe = std::fs::canonicalize(&out).unwrap().with_extension("");
        let st = std::process::Command::new("rustc")
            .args(["-O", "-C", "opt-level=3", "-C", "codegen-units=1", &out, "-o"]).arg(&exe).status();
        match st {
            Ok(s) if s.success() => {
                let s = std::process::Command::new(&exe).status().expect("無法執行程式");
                std::process::exit(s.code().unwrap_or(1));
            }
            Ok(s) => std::process::exit(s.code().unwrap_or(1)),
            Err(e) => { eprintln!("找不到 rustc: {}", e); std::process::exit(1); }
        }
    }
}