// Lambda desugaring (before monomorphization): each lambda expression is
// lifted into a generated top-level function (named `<enclosing>__lambdaN`)
// whose first parameter is a `*int` capture block, and the expression is
// replaced with a `Closure` (a pointer to a fresh `[fn_ptr, cap0, ...]`
// block).
//
// Captures are by-value snapshots taken when the closure is created:
//   - scalar captures (int / bool / string / pointer / array) are read in
//     the lifted function as `ctx[i]`;
//   - class captures (including `this`) are shadowed in the lifted function
//     by a local `T name = mem.retainVal(ctx[i]);` so each call owns an
//     independent reference for the call's duration (the block's own
//     reference is never dropped in v1).
//
// Calling convention: `fn(ctx, arg1, arg2)` — invoked via `sys.fnCall2`.
//
// v1 restrictions (diagnosed): no nested lambdas; no lambdas inside generic
// definitions (class or function); no `super` in lambdas; at most 5 lambda
// parameters (the ctx parameter occupies one slot).

use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::span::Span;
use std::collections::{HashMap, HashSet};

/// The (package, name) -> symbol name mapping, kept in sync with
/// `middle::mono::mangle` (the lifted functions are non-generic, so mono
/// mangles them identically).
fn mangle_symbol(package: &str, name: &str) -> String {
    if name == "main" {
        return "main".to_string();
    }
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{}_{}", package.replace('.', "_"), name)
    }
}

fn fqn(package: &str, name: &str) -> String {
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", package, name)
    }
}

/// Name lookups over the pre-monomorphization program (an immutable
/// snapshot; the desugaring mutates a clone's bodies in place).
struct Lookup<'a> {
    prog: &'a Program,
    class_by_name: HashMap<String, usize>,
    func_by_name: HashMap<String, usize>,
}

impl<'a> Lookup<'a> {
    fn new(prog: &'a Program) -> Self {
        let mut class_by_name = HashMap::new();
        let mut func_by_name = HashMap::new();
        for (i, s) in prog.structs.iter().enumerate() {
            class_by_name.insert(fqn(&s.package, &s.name), i);
            class_by_name.entry(s.name.clone()).or_insert(i);
        }
        for (i, f) in prog.funcs.iter().enumerate() {
            func_by_name.insert(fqn(&f.package, &f.name), i);
            func_by_name.entry(f.name.clone()).or_insert(i);
        }
        Lookup {
            prog,
            class_by_name,
            func_by_name,
        }
    }

    /// A lightweight type inference for a decl's value expression (the
    /// declared type wins; this fills in the rest for capture typing).
    fn expr_ty(&self, known: &HashMap<String, Ty>, e: &Expr) -> Option<Ty> {
        match e {
            Expr::Int { .. } | Expr::Bool { .. } | Expr::EnumVariant { .. } => Some(Ty::Int),
            Expr::Str { .. } => Some(Ty::Str),
            Expr::Null { .. } => Some(Ty::Ptr(None)),
            Expr::ArrayLit { .. } => Some(Ty::Array),
            Expr::Ident { name, .. } => known.get(name).cloned(),
            Expr::StructLit { name, type_args, .. } => {
                let ci = *self.class_by_name.get(name)?;
                if type_args.is_empty() {
                    Some(Ty::Struct(ci))
                } else {
                    Some(Ty::Inst(ci, type_args.clone()))
                }
            }
            Expr::Call { callee, .. } => {
                let joined = callee.join(".");
                let fi = *self.func_by_name.get(&joined)?;
                self.prog.funcs[fi].ret.clone()
            }
            Expr::MethodCall { base, method, .. } => {
                if let Expr::Ident { name, .. } = base.as_ref() {
                    let ci = *self.class_by_name.get(name)?;
                    let mut cur: Option<usize> = Some(ci);
                    while let Some(c) = cur {
                        if let Some(m) = self.prog.structs[c]
                            .methods
                            .iter()
                            .find(|m| &m.name == method)
                        {
                            return m.ret.clone();
                        }
                        cur = crate::backend::layout::parent_idx(self.prog, c);
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// A local of the enclosing function/method: name + type (declared or
/// inferred). Ordered: `this` (instance methods), parameters, then
/// declarations in first-occurrence source order. (Flint locals are
/// function-scoped — a name binds everywhere in the function.)
struct Scope {
    locals: Vec<(String, Option<Ty>)>,
}

fn register_local(
    known: &mut HashMap<String, Ty>,
    seen: &mut HashSet<String>,
    locals: &mut Vec<(String, Option<Ty>)>,
    name: &str,
    ty: Option<Ty>,
) {
    if seen.contains(name) {
        return;
    }
    seen.insert(name.to_string());
    if let Some(t) = &ty {
        known.insert(name.to_string(), t.clone());
    }
    locals.push((name.to_string(), ty));
}

fn build_scope(
    lk: &Lookup,
    has_this: bool,
    this_idx: usize,
    params: &[(String, Option<Ty>, Span, Option<Expr>)],
    body: &Block,
) -> Scope {
    let mut known: HashMap<String, Ty> = HashMap::new();
    let mut locals: Vec<(String, Option<Ty>)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if has_this {
        register_local(&mut known, &mut seen, &mut locals, "this", Some(Ty::Struct(this_idx)));
    }
    for (p, t, _, _) in params {
        register_local(&mut known, &mut seen, &mut locals, p, t.clone());
    }
    fn walk(
        lk: &Lookup,
        known: &mut HashMap<String, Ty>,
        seen: &mut HashSet<String>,
        locals: &mut Vec<(String, Option<Ty>)>,
        s: &Stmt,
    ) {
        match s {
            Stmt::Decl { name, ty, value, .. } => {
                register_local(
                    known,
                    seen,
                    locals,
                    name,
                    ty.clone().or_else(|| lk.expr_ty(known, value)),
                );
            }
            Stmt::TryCatch {
                catch_type,
                catch_var,
                try_block,
                catch_block,
                finally,
                ..
            } => {
                register_local(known, seen, locals, catch_var, Some(catch_type.clone()));
                walk_block(lk, known, seen, locals, try_block);
                walk_block(lk, known, seen, locals, catch_block);
                if let Some(f) = finally {
                    walk_block(lk, known, seen, locals, f);
                }
            }
            Stmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    walk(lk, known, seen, locals, i);
                }
                if let Some(u) = update {
                    walk(lk, known, seen, locals, u);
                }
                walk_block(lk, known, seen, locals, body);
            }
            Stmt::If { then, else_opt, .. } => {
                walk_block(lk, known, seen, locals, then);
                if let Some(e) = else_opt {
                    walk_block(lk, known, seen, locals, e);
                }
            }
            Stmt::While { body, .. } => walk_block(lk, known, seen, locals, body),
            Stmt::Switch {
                cases, default, ..
            } => {
                for (_, b2) in cases {
                    walk_block(lk, known, seen, locals, b2);
                }
                if let Some(d) = default {
                    walk_block(lk, known, seen, locals, d);
                }
            }
            _ => {}
        }
    }
    fn walk_block(
        lk: &Lookup,
        known: &mut HashMap<String, Ty>,
        seen: &mut HashSet<String>,
        locals: &mut Vec<(String, Option<Ty>)>,
        b: &Block,
    ) {
        for s in &b.stmts {
            walk(lk, known, seen, locals, s);
        }
    }
    walk_block(lk, &mut known, &mut seen, &mut locals, body);
    Scope { locals }
}

/// True when any Lambda node occurs in the block (or below).
fn block_has_lambda(b: &Block) -> bool {
    fn stmt_has(s: &Stmt) -> bool {
        match s {
            Stmt::Decl { value, .. } => expr_has(value),
            Stmt::Assign { value, .. } => expr_has(value),
            Stmt::ExprStmt { expr, .. } => expr_has(expr),
            Stmt::If { cond, then, else_opt, .. } => {
                expr_has(cond.as_ref())
                    || block_has_lambda(then)
                    || else_opt.as_ref().map_or(false, |e| block_has_lambda(e))
            }
            Stmt::While { cond, body, .. } => expr_has(cond.as_ref()) || block_has_lambda(body),
            Stmt::For {
                init, cond, update, body, ..
            } => {
                init.as_ref().map_or(false, |i| stmt_has(i))
                    || cond.as_ref().map_or(false, |c| expr_has(c))
                    || update.as_ref().map_or(false, |u| stmt_has(u))
                    || block_has_lambda(body)
            }
            Stmt::Return { value, .. } => value.as_ref().map_or(false, |v| expr_has(v)),
            Stmt::Throw { value, .. } => expr_has(value),
            Stmt::Switch {
                target, cases, default, ..
            } => {
                expr_has(target.as_ref())
                    || cases.iter().any(|(_, b)| block_has_lambda(b))
                    || default.as_ref().map_or(false, |d| block_has_lambda(d))
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                finally,
                ..
            } => {
                block_has_lambda(try_block)
                    || block_has_lambda(catch_block)
                    || finally.as_ref().map_or(false, |f| block_has_lambda(f))
            }
            _ => false,
        }
    }
    fn expr_has(e: &Expr) -> bool {
        match e {
            Expr::Lambda { .. } => true,
            Expr::Call { args, .. } => args.iter().any(expr_has),
            Expr::MethodCall { base, args, .. } => {
                expr_has(base.as_ref()) || args.iter().any(expr_has)
            }
            Expr::BinOp { l, r, .. } => expr_has(l.as_ref()) || expr_has(r.as_ref()),
            Expr::UnOp { e, .. }
            | Expr::IncrDecr { e, .. }
            | Expr::AddrOf { e, .. }
            | Expr::Deref { e, .. }
            | Expr::FnAddr { e, .. }
            | Expr::Await { e, .. }
            | Expr::Cast { e, .. }
            | Expr::Instanceof { e, .. } => expr_has(e.as_ref()),
            Expr::Index { base, idx, .. } => expr_has(base.as_ref()) || expr_has(idx.as_ref()),
            Expr::Field { base, .. } => expr_has(base.as_ref()),
            Expr::StructLit { fields, .. } => fields.iter().any(|(_, v)| expr_has(v)),
            Expr::ArrayLit { elems, .. } => elems.iter().any(expr_has),
            Expr::Cond { cond, then, els, .. } => {
                expr_has(cond.as_ref()) || expr_has(then.as_ref()) || expr_has(els.as_ref())
            }
            _ => false,
        }
    }
    b.stmts.iter().any(stmt_has)
}

/// Names declared anywhere in the block (flint locals are function-scoped,
/// so they bind everywhere in it).
fn declared_names(b: &Block) -> HashSet<String> {
    let mut out = HashSet::new();
    fn walk(s: &Stmt, out: &mut HashSet<String>) {
        match s {
            Stmt::Decl { name, .. } => {
                out.insert(name.clone());
            }
            Stmt::TryCatch {
                catch_var,
                try_block,
                catch_block,
                finally,
                ..
            } => {
                out.insert(catch_var.clone());
                walk_block(try_block, out);
                walk_block(catch_block, out);
                if let Some(f) = finally {
                    walk_block(f, out);
                }
            }
            Stmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    walk(i, out);
                }
                if let Some(u) = update {
                    walk(u, out);
                }
                walk_block(body, out);
            }
            Stmt::If { then, else_opt, .. } => {
                walk_block(then, out);
                if let Some(e) = else_opt {
                    walk_block(e, out);
                }
            }
            Stmt::While { body, .. } => walk_block(body, out),
            Stmt::Switch {
                cases, default, ..
            } => {
                for (_, b2) in cases {
                    walk_block(b2, out);
                }
                if let Some(d) = default {
                    walk_block(d, out);
                }
            }
            _ => {}
        }
    }
    fn walk_block(b: &Block, out: &mut HashSet<String>) {
        for s in &b.stmts {
            walk(s, out);
        }
    }
    walk_block(b, &mut out);
    out
}

/// The names free in the lambda body (its own parameters and declarations
/// are not free), plus whether a bare `this` is used. Also diagnoses the
/// v1 restrictions: nested lambdas and `super` in lambdas.
fn free_names(
    body: &Block,
    params: &[(String, Option<Ty>, Span)],
    span: Span,
) -> CompileResult<(HashSet<String>, bool)> {
    let declared = declared_names(body);
    let mut free: HashSet<String> = HashSet::new();
    let mut uses_this = false;
    fn walk(
        free: &mut HashSet<String>,
        this: &mut bool,
        declared: &HashSet<String>,
        params: &[(String, Option<Ty>, Span)],
        e: &Expr,
        span: Span,
    ) -> CompileResult<()> {
        match e {
            Expr::Ident { name, .. } => {
                if !declared.contains(name) && !params.iter().any(|(p, _, _)| p == name) {
                    free.insert(name.clone());
                }
            }
            Expr::This { .. } => *this = true,
            Expr::Lambda { .. } => {
                return Err(CompileError::new(span, "nested lambdas are not supported"));
            }
            Expr::SuperBase { .. } | Expr::SuperCall { .. } => {
                return Err(CompileError::new(span, "lambdas cannot use 'super'"));
            }
            Expr::Call { args, .. } => {
                for a in args {
                    walk(free, this, declared, params, a, span)?;
                }
            }
            Expr::MethodCall { base, args, .. } => {
                walk(free, this, declared, params, base, span)?;
                for a in args {
                    walk(free, this, declared, params, a, span)?;
                }
            }
            Expr::BinOp { l, r, .. } => {
                walk(free, this, declared, params, l, span)?;
                walk(free, this, declared, params, r, span)?;
            }
            Expr::UnOp { e: b, .. }
            | Expr::IncrDecr { e: b, .. }
            | Expr::AddrOf { e: b, .. }
            | Expr::Deref { e: b, .. }
            | Expr::FnAddr { e: b, .. }
            | Expr::Await { e: b, .. }
            | Expr::Cast { e: b, .. }
            | Expr::Instanceof { e: b, .. } => {
                walk(free, this, declared, params, b, span)?;
            }
            Expr::Index { base, idx, .. } => {
                walk(free, this, declared, params, base, span)?;
                walk(free, this, declared, params, idx, span)?;
            }
            Expr::Field { base, .. } => walk(free, this, declared, params, base, span)?,
            Expr::StructLit { fields, .. } => {
                for (_, v) in fields {
                    walk(free, this, declared, params, v, span)?;
                }
            }
            Expr::ArrayLit { elems, .. } => {
                for v in elems {
                    walk(free, this, declared, params, v, span)?;
                }
            }
            Expr::Cond { cond, then, els, .. } => {
                walk(free, this, declared, params, cond, span)?;
                walk(free, this, declared, params, then, span)?;
                walk(free, this, declared, params, els, span)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn walk_block(
        free: &mut HashSet<String>,
        this: &mut bool,
        declared: &HashSet<String>,
        params: &[(String, Option<Ty>, Span)],
        b: &Block,
        span: Span,
    ) -> CompileResult<()> {
        for s in &b.stmts {
            match s {
                Stmt::Decl { value, .. } => walk(free, this, declared, params, value, span)?,
                Stmt::Assign { value, .. } => walk(free, this, declared, params, value, span)?,
                Stmt::ExprStmt { expr, .. } => walk(free, this, declared, params, expr, span)?,
                Stmt::If {
                    cond, then, else_opt, ..
                } => {
                    walk(free, this, declared, params, cond, span)?;
                    walk_block(free, this, declared, params, then, span)?;
                    if let Some(e) = else_opt {
                        walk_block(free, this, declared, params, e, span)?;
                    }
                }
                Stmt::While { cond, body, .. } => {
                    walk(free, this, declared, params, cond, span)?;
                    walk_block(free, this, declared, params, body, span)?;
                }
                Stmt::For {
                    init, cond, update, body, ..
                } => {
                    if let Some(i) = init {
                        match i.as_ref() {
                            Stmt::Decl { value, .. } => {
                                walk(free, this, declared, params, value, span)?
                            }
                            _ => {}
                        }
                    }
                    if let Some(c) = cond {
                        walk(free, this, declared, params, c, span)?;
                    }
                    walk_block(free, this, declared, params, body, span)?;
                    if let Some(u) = update {
                        match u.as_ref() {
                            Stmt::Assign { value, .. }
                            | Stmt::CompoundAssign { value, .. } => {
                                walk(free, this, declared, params, value, span)?
                            }
                            Stmt::ExprStmt { expr, .. } => {
                                walk(free, this, declared, params, expr, span)?
                            }
                            _ => {}
                        }
                    }
                }
                Stmt::Return { value, .. } => {
                    if let Some(v) = value {
                        walk(free, this, declared, params, v, span)?;
                    }
                }
                Stmt::Throw { value, .. } => walk(free, this, declared, params, value, span)?,
                Stmt::Switch {
                    target, cases, default, ..
                } => {
                    walk(free, this, declared, params, target, span)?;
                    for (_, b2) in cases {
                        walk_block(free, this, declared, params, b2, span)?;
                    }
                    if let Some(d) = default {
                        walk_block(free, this, declared, params, d, span)?;
                    }
                }
                Stmt::TryCatch {
                    try_block,
                    catch_block,
                    finally,
                    ..
                } => {
                    walk_block(free, this, declared, params, try_block, span)?;
                    walk_block(free, this, declared, params, catch_block, span)?;
                    if let Some(f) = finally {
                        walk_block(free, this, declared, params, f, span)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    walk_block(&mut free, &mut uses_this, &declared, params, body, span)?;
    Ok((free, uses_this))
}

/// Rewrite the lambda body for the lifted function: scalar captures become
/// `ctx[i]`; a class `this` capture becomes the `.lthis` shadow local;
/// class-typed named captures keep their name (a shadow decl holds the
/// value).
fn rewrite_body(e: &mut Expr, scalar_off: &HashMap<String, i64>, this_captured: bool) {
    match e {
        Expr::Ident { name, span, .. } => {
            if let Some(&off) = scalar_off.get(name) {
                *e = Expr::Index {
                    span: *span,
                    base: Box::new(Expr::Ident {
                        span: *span,
                        name: "ctx".to_string(),
                    }),
                    idx: Box::new(Expr::Int {
                        span: *span,
                        value: off,
                    }),
                };
            }
        }
        Expr::This { span, .. } => {
            if this_captured {
                *e = Expr::Ident {
                    span: *span,
                    name: ".lthis".to_string(),
                };
            }
        }
        Expr::Call { args, .. } => {
            for a in args.iter_mut() {
                rewrite_body(a, scalar_off, this_captured);
            }
        }
        Expr::MethodCall { base, args, .. } => {
            rewrite_body(base, scalar_off, this_captured);
            for a in args.iter_mut() {
                rewrite_body(a, scalar_off, this_captured);
            }
        }
        Expr::BinOp { l, r, .. } => {
            rewrite_body(l, scalar_off, this_captured);
            rewrite_body(r, scalar_off, this_captured);
        }
        Expr::UnOp { e: b, .. }
        | Expr::IncrDecr { e: b, .. }
        | Expr::AddrOf { e: b, .. }
        | Expr::Deref { e: b, .. }
        | Expr::FnAddr { e: b, .. }
        | Expr::Await { e: b, .. }
        | Expr::Cast { e: b, .. }
        | Expr::Instanceof { e: b, .. } => {
            rewrite_body(b, scalar_off, this_captured);
        }
        Expr::Index { base, idx, .. } => {
            rewrite_body(base, scalar_off, this_captured);
            rewrite_body(idx, scalar_off, this_captured);
        }
        Expr::Field { base, .. } => rewrite_body(base, scalar_off, this_captured),
        Expr::StructLit { fields, .. } => {
            for (_, v) in fields.iter_mut() {
                rewrite_body(v, scalar_off, this_captured);
            }
        }
        Expr::ArrayLit { elems, .. } => {
            for v in elems.iter_mut() {
                rewrite_body(v, scalar_off, this_captured);
            }
        }
        Expr::Cond { cond, then, els, .. } => {
            rewrite_body(cond, scalar_off, this_captured);
            rewrite_body(then, scalar_off, this_captured);
            rewrite_body(els, scalar_off, this_captured);
        }
        _ => {}
    }
}

/// Rewrite helper for statements (mirrors `rewrite_body` for expressions).
fn rewrite_stmt(s: &mut Stmt, scalar_off: &HashMap<String, i64>, this_captured: bool) {
    fn block(b: &mut Block, scalar_off: &HashMap<String, i64>, this_captured: bool) {
        for s2 in b.stmts.iter_mut() {
            stmt(s2, scalar_off, this_captured);
        }
    }
    fn stmt(s: &mut Stmt, scalar_off: &HashMap<String, i64>, this_captured: bool) {
        match s {
            Stmt::Decl { value, .. } => rewrite_body(value, scalar_off, this_captured),
            Stmt::Assign { value, .. } => rewrite_body(value, scalar_off, this_captured),
            Stmt::ExprStmt { expr: e, .. } => rewrite_body(e, scalar_off, this_captured),
            Stmt::If {
                cond, then, else_opt, ..
            } => {
                rewrite_body(cond, scalar_off, this_captured);
                block(then, scalar_off, this_captured);
                if let Some(e) = else_opt {
                    block(e, scalar_off, this_captured);
                }
            }
            Stmt::While { cond, body, .. } => {
                rewrite_body(cond, scalar_off, this_captured);
                block(body, scalar_off, this_captured);
            }
            Stmt::For {
                init, cond, update, body, ..
            } => {
                if let Some(i) = init {
                    stmt(i, scalar_off, this_captured);
                }
                if let Some(c) = cond {
                    rewrite_body(c, scalar_off, this_captured);
                }
                block(body, scalar_off, this_captured);
                if let Some(u) = update {
                    stmt(u, scalar_off, this_captured);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    rewrite_body(v, scalar_off, this_captured);
                }
            }
            Stmt::Throw { value, .. } => rewrite_body(value, scalar_off, this_captured),
            Stmt::Switch {
                target, cases, default, ..
            } => {
                rewrite_body(target, scalar_off, this_captured);
                for (_, b2) in cases.iter_mut() {
                    block(b2, scalar_off, this_captured);
                }
                if let Some(d) = default {
                    block(d, scalar_off, this_captured);
                }
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                finally,
                ..
            } => {
                block(try_block, scalar_off, this_captured);
                block(catch_block, scalar_off, this_captured);
                if let Some(fb) = finally {
                    block(fb, scalar_off, this_captured);
                }
            }
            _ => {}
        }
    }
    stmt(s, scalar_off, this_captured);
}

/// True when a type is a managed (reference-counted) object.
fn is_class_ty(t: &Option<Ty>) -> bool {
    matches!(
        t,
        Some(Ty::Struct(_)) | Some(Ty::Inst(_, _)) | Some(Ty::Interface(_))
    )
}

/// Desugar every lambda in the program.
pub fn desugar_lambdas(prog: &Program) -> CompileResult<Program> {
    let mut out = prog.clone();
    let lk = Lookup::new(prog);
    let mut lifted: Vec<FuncDef> = Vec::new();

    for (ci, s) in out.structs.iter_mut().enumerate() {
        let pkg = s.package.clone();
        let cname = s.name.clone();
        for m in s.methods.iter_mut() {
            lift_def(
                &lk,
                &pkg,
                &format!("{}_{}", cname, m.name),
                &s.type_params,
                !m.is_static,
                ci,
                &m.params,
                &mut m.body,
                &mut lifted,
            )?;
        }
    }
    for f in out.funcs.iter_mut() {
        let pkg = f.package.clone();
        let fname = f.name.clone();
        lift_def(
            &lk,
            &pkg,
            &fname,
            &f.type_params,
            false,
            0,
            &f.params,
            &mut f.body,
            &mut lifted,
        )?;
    }
    out.funcs.extend(lifted);
    Ok(out)
}

/// Lift the lambdas of one function/method body.
fn lift_def(
    lk: &Lookup,
    pkg: &str,
    base: &str,
    type_params: &[String],
    has_this: bool,
    this_idx: usize,
    params: &[(String, Option<Ty>, Span, Option<Expr>)],
    body: &mut Block,
    lifted: &mut Vec<FuncDef>,
) -> CompileResult<()> {
    if !type_params.is_empty() && block_has_lambda(body) {
        return Err(CompileError::new(
            body.span,
            "lambdas are not yet supported inside generic definitions",
        ));
    }
    if !block_has_lambda(body) {
        return Ok(());
    }
    let scope = build_scope(lk, has_this, this_idx, params, body);
    let mut n = 0;
    desugar_block(lk, pkg, base, &scope, body, lifted, &mut n)
}

/// Walk one block, lifting lambdas (with full error propagation).
fn desugar_block(
    _lk: &Lookup,
    pkg: &str,
    base: &str,
    scope: &Scope,
    b: &mut Block,
    lifted: &mut Vec<FuncDef>,
    n: &mut usize,
) -> CompileResult<()> {
    for s in b.stmts.iter_mut() {
        desugar_stmt(pkg, base, scope, s, lifted, n)?;
    }
    Ok(())
}

fn desugar_stmt(
    pkg: &str,
    base: &str,
    scope: &Scope,
    s: &mut Stmt,
    lifted: &mut Vec<FuncDef>,
    n: &mut usize,
) -> CompileResult<()> {
    fn expr(
        pkg: &str,
        base: &str,
        scope: &Scope,
        e: &mut Expr,
        lifted: &mut Vec<FuncDef>,
        n: &mut usize,
    ) -> CompileResult<()> {
        match e {
            Expr::Lambda {
                span,
                params: lparams,
                ret,
                body: lbody,
            } => {
                let (free, uses_this) = free_names(lbody, lparams, *span)?;
                if 1 + lparams.len() > 6 {
                    return Err(CompileError::new(
                        *span,
                        "lambdas may have at most 5 parameters (the capture context takes one slot)",
                    ));
                }
                // Captures in canonical order: `this`, parameters, then
                // declarations (first-occurrence order).
                let mut captures: Vec<(String, Option<Ty>, bool, bool)> = Vec::new();
                let mut free = free;
                for (name, ty) in &scope.locals {
                    if name == "this" {
                        if uses_this {
                            captures.push((name.clone(), ty.clone(), true, true));
                        }
                        continue;
                    }
                    if free.remove(name) {
                        captures.push((name.clone(), ty.clone(), false, is_class_ty(ty)));
                    }
                }
                // Shadow decls for the class captures (including `this` ->
                // `.lthis`), prepended to the lifted body; scalar captures
                // are read as `ctx[i]`.
                let mut shadow: Vec<Stmt> = Vec::new();
                let mut scalar_off: HashMap<String, i64> = HashMap::new();
                let mut slot = 1; // slot 0 = the function pointer
                let mut this_captured = false;
                for (name, ty, is_this, is_class) in &captures {
                    let sym = if *is_this {
                        ".lthis".to_string()
                    } else {
                        name.clone()
                    };
                    if *is_class {
                        let dspan = Span::new(
                            span.start + 100 + slot as usize * 3,
                            span.start + 100 + slot as usize * 3 + 2,
                        );
                        shadow.push(Stmt::Decl {
                            span: dspan,
                            name: sym,
                            ty: ty.clone(),
                            value: Expr::Call {
                                span: dspan,
                                callee: vec!["mem".to_string(), "retainVal".to_string()],
                                type_args: Vec::new(),
                                args: vec![Expr::Index {
                                    span: dspan,
                                    base: Box::new(Expr::Ident {
                                        span: dspan,
                                        name: "ctx".to_string(),
                                    }),
                                    idx: Box::new(Expr::Int {
                                        span: dspan,
                                        value: slot as i64,
                                    }),
                                }],
                            },
                        });
                        if *is_this {
                            this_captured = true;
                        }
                    } else {
                        scalar_off.insert(name.clone(), slot as i64);
                    }
                    slot += 1;
                }
                // Rewrite the body for the lifted function.
                let mut lbody = std::mem::replace(
                    lbody,
                    Block {
                        span: *span,
                        stmts: Vec::new(),
                    },
                );
                for s2 in lbody.stmts.iter_mut() {
                    rewrite_stmt(s2, &scalar_off, this_captured);
                }
                let name = format!("{}__lambda{}", base, n);
                let fn_name = mangle_symbol(pkg, &name);
                let params: Vec<(String, Option<Ty>, Span, Option<Expr>)> = {
                    let mut p = vec![("ctx".to_string(), Some(Ty::Ptr(None)), *span, None)];
                    p.extend(lparams.iter().map(|(n, t, s)| (n.clone(), t.clone(), *s, None)));
                    p
                };
                let mut body2 = Block {
                    span: *span,
                    stmts: shadow,
                };
                body2.stmts.append(&mut lbody.stmts);
                lifted.push(FuncDef {
                    span: *span,
                    package: pkg.to_string(),
                    name,
                    type_params: Vec::new(),
                    is_async: false,
                    params,
                    ret: ret.clone(),
                    body: body2,
                });
                // The replacement: a closure value.
                let mut cap_exprs: Vec<Expr> = Vec::new();
                for (name, _, is_this, _) in &captures {
                    if *is_this {
                        cap_exprs.push(Expr::This { span: *span });
                    } else {
                        cap_exprs.push(Expr::Ident {
                            span: *span,
                            name: name.clone(),
                        });
                    }
                }
                *e = Expr::Closure {
                    span: *span,
                    fn_name,
                    captures: cap_exprs,
                };
                *n += 1;
            }
            Expr::Call { args, .. } => {
                for a in args.iter_mut() {
                    expr(pkg, base, scope, a, lifted, n)?;
                }
            }
            Expr::MethodCall { base: b2, args, .. } => {
                expr(pkg, base, scope, b2, lifted, n)?;
                for a in args.iter_mut() {
                    expr(pkg, base, scope, a, lifted, n)?;
                }
            }
            Expr::BinOp { l, r, .. } => {
                expr(pkg, base, scope, l, lifted, n)?;
                expr(pkg, base, scope, r, lifted, n)?;
            }
            Expr::UnOp { e: b, .. }
            | Expr::IncrDecr { e: b, .. }
            | Expr::AddrOf { e: b, .. }
            | Expr::Deref { e: b, .. }
            | Expr::FnAddr { e: b, .. }
            | Expr::Await { e: b, .. }
            | Expr::Cast { e: b, .. }
            | Expr::Instanceof { e: b, .. } => {
                expr(pkg, base, scope, b, lifted, n)?;
            }
            Expr::Index { base: b2, idx, .. } => {
                expr(pkg, base, scope, b2, lifted, n)?;
                expr(pkg, base, scope, idx, lifted, n)?;
            }
            Expr::Field { base: b2, .. } => {
                expr(pkg, base, scope, b2, lifted, n)?;
            }
            Expr::StructLit { fields, .. } => {
                for (_, v) in fields.iter_mut() {
                    expr(pkg, base, scope, v, lifted, n)?;
                }
            }
            Expr::ArrayLit { elems, .. } => {
                for v in elems.iter_mut() {
                    expr(pkg, base, scope, v, lifted, n)?;
                }
            }
            Expr::Cond {
                cond, then, els, ..
            } => {
                expr(pkg, base, scope, cond, lifted, n)?;
                expr(pkg, base, scope, then, lifted, n)?;
                expr(pkg, base, scope, els, lifted, n)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn block(
        pkg: &str,
        base: &str,
        scope: &Scope,
        b: &mut Block,
        lifted: &mut Vec<FuncDef>,
        n: &mut usize,
    ) -> CompileResult<()> {
        for s2 in b.stmts.iter_mut() {
            desugar_stmt(pkg, base, scope, s2, lifted, n)?;
        }
        Ok(())
    }
    match s {
        Stmt::Decl { value, .. } => {
            expr(pkg, base, scope, value, lifted, n)?;
        }
        Stmt::Assign { value, .. } => {
            expr(pkg, base, scope, value, lifted, n)?;
        }
        Stmt::ExprStmt { expr: e, .. } => {
            expr(pkg, base, scope, e, lifted, n)?;
        }
        Stmt::If {
            cond, then, else_opt, ..
        } => {
            expr(pkg, base, scope, cond, lifted, n)?;
            block(pkg, base, scope, then, lifted, n)?;
            if let Some(e) = else_opt {
                block(pkg, base, scope, e, lifted, n)?;
            }
        }
        Stmt::While { cond, body: b2, .. } => {
            expr(pkg, base, scope, cond, lifted, n)?;
            block(pkg, base, scope, b2, lifted, n)?;
        }
        Stmt::For {
            init, cond, update, body: b2, ..
        } => {
            if let Some(i) = init {
                desugar_stmt(pkg, base, scope, i, lifted, n)?;
            }
            if let Some(c) = cond {
                expr(pkg, base, scope, c, lifted, n)?;
            }
            block(pkg, base, scope, b2, lifted, n)?;
            if let Some(u) = update {
                desugar_stmt(pkg, base, scope, u, lifted, n)?;
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                expr(pkg, base, scope, v, lifted, n)?;
            }
        }
        Stmt::Throw { value, .. } => {
            expr(pkg, base, scope, value, lifted, n)?;
        }
        Stmt::Switch {
            target, cases, default, ..
        } => {
            expr(pkg, base, scope, target, lifted, n)?;
            for (_, b2) in cases.iter_mut() {
                block(pkg, base, scope, b2, lifted, n)?;
            }
            if let Some(d) = default {
                block(pkg, base, scope, d, lifted, n)?;
            }
        }
        Stmt::TryCatch {
            try_block,
            catch_block,
            finally,
            ..
        } => {
            block(pkg, base, scope, try_block, lifted, n)?;
            block(pkg, base, scope, catch_block, lifted, n)?;
            if let Some(fb) = finally {
                block(pkg, base, scope, fb, lifted, n)?;
            }
        }
        _ => {}
    }
    Ok(())
}
