use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use super::Mono;

impl<'a> Mono<'a> {
    /// Substitute type parameters and resolve nested instantiations.
    pub(crate) fn resolve_type(&mut self, ty: &Ty, subst: &[(String, Ty)]) -> CompileResult<Ty> {
        match ty {
            Ty::Param(n) => {
                for (pn, pt) in subst {
                    if pn == n {
                        // `pt` is already a resolved type (a concrete type or an
                        // expanded `Ty::Struct` output index); substitute it in
                        // directly without re-resolving.
                        return Ok(pt.clone());
                    }
                }
                Err(CompileError::new(
                    Span::new(0, 0),
                    format!("unresolved type parameter '{}'", n),
                ))
            }
            Ty::Inst(idx, args) => {
                let resolved: Vec<Ty> = args
                    .iter()
                    .map(|a| self.resolve_type(a, subst))
                    .collect::<CompileResult<_>>()?;
                let ni = self.ensure_class(*idx, resolved)?;
                Ok(Ty::Struct(ni))
            }
            Ty::Struct(idx) => {
                let (is_generic, name) = {
                    let s = &self.prog.structs[*idx];
                    (!s.type_params.is_empty(), s.name.clone())
                };
                if is_generic {
                    Err(CompileError::new(
                        Span::new(0, 0),
                        format!("type arguments required for generic class '{}'", name),
                    ))
                } else {
                    Ok(Ty::Struct(self.struct_new[idx]))
                }
            }
            Ty::Ptr(inner) => {
                let ni = match inner {
                    Some(t) => Some(Box::new(self.resolve_type(t, subst)?)),
                    None => None,
                };
                Ok(Ty::Ptr(ni))
            }
            other => Ok(other.clone()),
        }
    }

    pub(crate) fn resolve_block(&mut self, block: &Block, subst: &[(String, Ty)]) -> CompileResult<Block> {
        let mut stmts = Vec::new();
        for s in &block.stmts {
            stmts.extend(self.resolve_stmt(s, subst)?);
        }
        Ok(Block {
            span: block.span,
            stmts,
        })
    }

    /// Resolve one statement. Most statements resolve to themselves;
    /// `ForEach` desugars into the run of statements that implement it.
    pub(crate) fn resolve_stmt(
        &mut self,
        stmt: &Stmt,
        subst: &[(String, Ty)],
    ) -> CompileResult<Vec<Stmt>> {
        match stmt {
            Stmt::Decl { span, name, ty, value } => {
                let value = self.resolve_expr(value, subst)?;
                let ty = match ty {
                    Some(t) => Some(self.resolve_type(t, subst)?),
                    // `var`: infer from the initializer (best effort; a
                    // failure leaves the type unresolved for the backend's
                    // own fallback, which defaults to int).
                    None => self.expr_type(&value, subst).ok(),
                };
                if let Some(t) = &ty {
                    self.cur_locals.insert(name.clone(), t.clone());
                }
                Ok(vec![Stmt::Decl {
                    span: *span,
                    name: name.clone(),
                    ty,
                    value,
                }])
            }
            Stmt::Assign { span, target, value } => {
                let target = self.resolve_expr(target, subst)?;
                let value = self.resolve_expr(value, subst)?;
                Ok(vec![Stmt::Assign {
                    span: *span,
                    target,
                    value,
                }])
            }
            Stmt::ExprStmt { span, expr } => {
                let expr = self.resolve_expr(expr, subst)?;
                Ok(vec![Stmt::ExprStmt { span: *span, expr }])
            }
            Stmt::If {
                span,
                cond,
                then,
                else_opt,
            } => {
                let cond = Box::new(self.resolve_expr(cond, subst)?);
                let then = Box::new(self.resolve_block(then, subst)?);
                let else_opt = match else_opt {
                    Some(b) => Some(Box::new(self.resolve_block(b, subst)?)),
                    None => None,
                };
                Ok(vec![Stmt::If {
                    span: *span,
                    cond,
                    then,
                    else_opt,
                }])
            }
            Stmt::While { span, label, cond, body } => {
                let cond = Box::new(self.resolve_expr(cond, subst)?);
                let body = Box::new(self.resolve_block(body, subst)?);
                Ok(vec![Stmt::While {
                    span: *span,
                    label: label.clone(),
                    cond,
                    body,
                }])
            }
            Stmt::For {
                span,
                label,
                init,
                cond,
                update,
                body,
            } => {
                let init = match init {
                    Some(s) => Some(Box::new(self.resolve_stmt(s, subst)?.pop().unwrap())),
                    None => None,
                };
                let cond = match cond {
                    Some(c) => Some(Box::new(self.resolve_expr(c, subst)?)),
                    None => None,
                };
                let update = match update {
                    Some(s) => Some(Box::new(self.resolve_stmt(s, subst)?.pop().unwrap())),
                    None => None,
                };
                let body = Box::new(self.resolve_block(body, subst)?);
                Ok(vec![Stmt::For {
                    span: *span,
                    label: label.clone(),
                    init,
                    cond,
                    update,
                    body,
                }])
            }
            Stmt::ForEach {
                span,
                label,
                name,
                ty,
                target,
                body,
                base_ty: _,
            } => {
                // Phase 1 (function context): resolve the parts that need this
                // function's context (types, target, body) and register the
                // loop variable best-effort. The index loop itself is built
                // by the post-expansion pass, because the target may be a
                // generic class instance that is only a placeholder here.
                let target = self.resolve_expr(target, subst)?;
                let ty = match ty {
                    Some(t) => Some(self.resolve_type(t, subst)?),
                    None => None,
                };
                let base_ty = self.expr_type(&target, subst).ok().or_else(|| {
                    // A method-call target whose class is still a
                    // placeholder (e.g. `m.keys()`): resolve the return
                    // type from the template.
                    if let Expr::MethodCall {
                        base, method, ..
                    } = &target
                    {
                        if let Expr::Ident { name, .. } = base.as_ref() {
                            if let Some(t) = self.placeholder_call_ret(name, method) {
                                return self.resolve_type(&t, subst).ok();
                            }
                        }
                    }
                    None
                });
                let elem = ty
                    .clone()
                    .or_else(|| base_ty.as_ref().and_then(|t| self.best_effort_elem(t)));
                if let Some(e) = elem {
                    self.cur_locals.insert(name.to_string(), e);
                }
                let body = self.resolve_block(body, subst)?;
                Ok(vec![Stmt::ForEach {
                    span: *span,
                    label: label.clone(),
                    name: name.to_string(),
                    ty,
                    base_ty,
                    target: Box::new(target),
                    body: Box::new(body),
                }])
            }
            Stmt::Return { span, value } => {
                let value = match value {
                    Some(v) => Some(Box::new(self.resolve_expr(v, subst)?)),
                    None => None,
                };
                Ok(vec![Stmt::Return { span: *span, value }])
            }
            Stmt::Break { span, label } => Ok(vec![Stmt::Break { span: *span, label: label.clone() }]),
            Stmt::Continue { span, label } => Ok(vec![Stmt::Continue { span: *span, label: label.clone() }]),
            Stmt::CompoundAssign {
                span,
                target,
                op,
                value,
            } => {
                let target = self.resolve_expr(target, subst)?;
                let value = self.resolve_expr(value, subst)?;
                Ok(vec![Stmt::CompoundAssign {
                    span: *span,
                    target,
                    op: *op,
                    value,
                }])
            }
            Stmt::Switch {
                span,
                target,
                cases,
                default,
            } => {
                let target = Box::new(self.resolve_expr(target, subst)?);
                let cases = cases
                    .iter()
                    .map(|(v, b)| {
                        let b = self.resolve_block(b, subst)?;
                        Ok((*v, b))
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                let default = match default {
                    Some(b) => Some(Box::new(self.resolve_block(b, subst)?)),
                    None => None,
                };
                Ok(vec![Stmt::Switch {
                    span: *span,
                    target,
                    cases,
                    default,
                }])
            }
            Stmt::Throw { span, value } => {
                let value = Box::new(self.resolve_expr(value, subst)?);
                Ok(vec![Stmt::Throw { span: *span, value }])
            }
            Stmt::TryCatch {
                span,
                try_block,
                catch_type,
                catch_var,
                catch_block,
                finally,
            } => {
                let try_block = Box::new(self.resolve_block(try_block, subst)?);
                let catch_block = Box::new(self.resolve_block(catch_block, subst)?);
                let finally = match finally {
                    Some(b) => Some(Box::new(self.resolve_block(b, subst)?)),
                    None => None,
                };
                Ok(vec![Stmt::TryCatch {
                    span: *span,
                    try_block,
                    catch_type: catch_type.clone(),
                    catch_var: catch_var.clone(),
                    catch_block,
                    finally,
                }])
            }
        }
    }

    /// Best-effort element type of an iteration target, for registering the
    /// loop variable in the function's local table during phase 1. `None`
    /// when the answer is not available yet (a generic class instance that
    /// is still a placeholder) — the post-expansion pass finalizes it.
    fn best_effort_elem(&self, t: &Ty) -> Option<Ty> {
        match t {
            Ty::Array => {
                // Array elements are plain 64-bit slots (rows of a
                // multi-dimensional array are themselves arrays).
                Some(Ty::Array)
            }
            Ty::Struct(s) => {
                if self.out.structs[*s].name == "__placeholder__" {
                    return None;
                }
                find_accessors(&self.out, *s).0
            }
            _ => None,
        }
    }

    /// Return type of `local.method(...)` when `local`'s class is still a
    /// placeholder: look the method up on the template and substitute the
    /// instantiation arguments for its type parameters.
    fn placeholder_call_ret(&self, base_name: &str, method: &str) -> Option<Ty> {
        let s = match self.cur_locals.get(base_name) {
            Some(Ty::Struct(s)) => *s,
            _ => return None,
        };
        if self.out.structs[s].name != "__placeholder__" {
            return None;
        }
        let (ti, args) = self
            .class_inst
            .iter()
            .find(|(_, ni)| **ni == s)?
            .0
            .clone();
        let template = &self.prog.structs[ti];
        let mut cur: Option<usize> = Some(ti);
        let mut hops = 0;
        while let Some(c) = cur {
            hops += 1;
            if hops > 16 {
                break;
            }
            if let Some(m) = self.prog.structs[c]
                .methods
                .iter()
                .find(|m| m.name == method)
            {
                let mut ty = m.ret.clone().unwrap_or(Ty::Int);
                for (p, a) in template.type_params.iter().zip(args.iter()) {
                    ty = replace_param(ty, p, a);
                }
                return Some(ty);
            }
            cur = match &self.prog.structs[c].extends {
                Some(en) => {
                    let short = en.rsplit('.').next().unwrap_or(en);
                    self.prog
                        .structs
                        .iter()
                        .position(|x| x.name == *en || x.name == short)
                        .or(None)
                }
                None => None,
            };
        }
        None
    }

}

/// Replace a type parameter with the instantiating argument, descending
/// into instantiated-generic and pointer types.
fn replace_param(t: Ty, p: &str, a: &Ty) -> Ty {
    match t {
        Ty::Param(n) if n == p => a.clone(),
        Ty::Inst(idx, args) => {
            let mut new_args = Vec::with_capacity(args.len());
            for x in args {
                new_args.push(replace_param(x.clone(), p, a));
            }
            Ty::Inst(idx, new_args)
        }
        Ty::Ptr(Some(inner)) => Ty::Ptr(Some(Box::new(replace_param(*inner, p, a)))),
        _ => t,
    }
}

/// A short type name for diagnostics.
fn ty_short(t: &Ty) -> String {
    match t {
        Ty::Int => "int".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Ptr(None) => "*ptr".to_string(),
        Ty::Ptr(Some(i)) => format!("*{}", ty_short(i)),
        Ty::Str => "string".to_string(),
        Ty::Array => "array".to_string(),
        Ty::Void => "void".to_string(),
        Ty::Enum(_) => "enum".to_string(),
        Ty::Interface(_) => "interface".to_string(),
        _ => "value".to_string(),
    }
}

/// Phase 2 of for-each desugaring, run on the fully expanded program
/// (after the monomorphizer's worklists have drained, so every generic
/// class instance is real). Replaces each `ForEach` with the index loop
/// that implements it.
pub fn desugar_for_eachs(prog: &mut Program) -> CompileResult<()> {
    for i in 0..prog.funcs.len() {
        let span = prog.funcs[i].span;
        let mut b = std::mem::replace(&mut prog.funcs[i].body, Block { span, stmts: Vec::new() });
        let mut n = 0usize;
        desugar_block(prog, &mut b, &mut n)?;
        prog.funcs[i].body = b;
    }
    for i in 0..prog.structs.len() {
        let mut ms = std::mem::take(&mut prog.structs[i].methods);
        for j in 0..ms.len() {
            let span = ms[j].span;
            let mut b = std::mem::replace(&mut ms[j].body, Block { span, stmts: Vec::new() });
            let mut n = 0usize;
            desugar_block(prog, &mut b, &mut n)?;
            ms[j].body = b;
        }
        prog.structs[i].methods = ms;
    }
    Ok(())
}

fn desugar_block(prog: &Program, block: &mut Block, n: &mut usize) -> CompileResult<()> {
    // Per-loop counter: the desugared temps must have unique names within
    // the function. A single shared `.rf_base` name would make the codegen
    // frame resolve every loop's reads to the *last* decl's slot, so a
    // class-typed base (e.g. `Queue`) would dispatch `get` through another
    // class's (e.g. `List`) vtable slot and call the wrong method.
    let mut out: Vec<Stmt> = Vec::new();
    for i in 0..block.stmts.len() {
        let mut s = block.stmts[i].clone();
        if matches!(&s, Stmt::ForEach { .. }) {
            let mut desug = desugar_for_each(prog, &s, *n)?;
            *n += 1;
            // The desugared body still needs its own nested desugaring.
            for d in desug.iter_mut() {
                desugar_stmt(prog, d, n)?;
            }
            out.extend(desug);
        } else {
            desugar_stmt(prog, &mut s, n)?;
            out.push(s);
        }
    }
    block.stmts = out;
    Ok(())
}

fn desugar_stmt(prog: &Program, s: &mut Stmt, n: &mut usize) -> CompileResult<()> {
    match s {
        Stmt::If {
            then, else_opt, ..
        } => {
            desugar_block(prog, then, n)?;
            if let Some(e) = else_opt {
                desugar_block(prog, e, n)?;
            }
        }
        Stmt::While { body, .. } => desugar_block(prog, body, n)?,
        Stmt::For {
            init, body, update, ..
        } => {
            if let Some(i) = init {
                desugar_stmt(prog, i, n)?;
            }
            desugar_block(prog, body, n)?;
            if let Some(u) = update {
                desugar_stmt(prog, u, n)?;
            }
        }
        Stmt::Switch {
            cases, default, ..
        } => {
            for (_, b) in cases {
                desugar_block(prog, b, n)?;
            }
            if let Some(d) = default {
                desugar_block(prog, d, n)?;
            }
        }
        Stmt::TryCatch {
            try_block,
            catch_block,
            finally,
            ..
        } => {
            desugar_block(prog, try_block, n)?;
            desugar_block(prog, catch_block, n)?;
            if let Some(f) = finally {
                desugar_block(prog, f, n)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Build the index loop for `for (T x : c) { ... }`. The target's type
/// picks the access path: arrays use `len` / `[]`; a collection (a class
/// exposing `size()` and `get(int)`) uses those methods. A `HashMap` is
/// rejected with a hint; other types are not iterable. The element type
/// is `T` when given, otherwise inferred from the target (the array's
/// element, or the collection's `get` return).
fn desugar_for_each(prog: &Program, fe: &Stmt, n: usize) -> CompileResult<Vec<Stmt>> {
    let Stmt::ForEach {
        span,
        label,
        name,
        ty,
        target,
        body,
        base_ty,
    } = fe
    else {
        unreachable!()
    };
    let base_ty = base_ty.clone().ok_or_else(|| {
        CompileError::new(
            *span,
            "cannot determine the type of the iteration target".to_string(),
        )
    })?;
    let (elem_ty, kind) = match &base_ty {
        // Array slots hold plain 64-bit ints; only an explicit `[]`
        // annotation (`int r[]`, `var row[]`) makes the loop variable an
        // array (for nested arrays). A bare `var x` infers `int`, so the
        // slot is never treated as an owned array by mistake.
        Ty::Array => (ty.clone().unwrap_or(Ty::Int), AccessKind::Array),
        Ty::Struct(s) => {
            let (get, size) = find_accessors(prog, *s);
            let get = match get {
                Some(g) => g,
                None => {
                    if is_map(prog, *s) {
                        return Err(CompileError::new(
                            *span,
                            format!(
                                "cannot iterate a {} directly; use .keys() or .values()",
                                prog.structs[*s].name
                            ),
                        ));
                    }
                    return Err(CompileError::new(
                        *span,
                        format!(
                            "cannot iterate a {} value; it has no size()/get(i) methods",
                            prog.structs[*s].name
                        ),
                    ));
                }
            };
            if size.is_none() {
                return Err(CompileError::new(
                    *span,
                    format!(
                        "cannot iterate a {} value; it has no size() method",
                        prog.structs[*s].name
                    ),
                ));
            }
            (ty.clone().unwrap_or(get), AccessKind::Collection)
        }
        other => {
            let s = ty_short(other);
            let art = if s.starts_with('i') || s.starts_with('e') {
                "an"
            } else {
                "a"
            };
            return Err(CompileError::new(
                *span,
                format!("cannot iterate {} {} value", art, s),
            ));
        }
    };
    let target = target.clone();
    let body = body.clone();
    let name = name.clone();
    let label = label.clone();

    let base_name = format!(".rf_base_{}", n);
    let idx_name = format!(".rf_i_{}", n);
    // The desugared decls need distinct spans: the escape plan keys decl
    // decisions by decl-statement span. Offsets inside the `for` keyword
    // can never collide with a real statement's span.
    let base_span = Span::new(span.start + 1, span.end + 1);
    let idx_span = Span::new(span.start + 2, span.end + 2);
    let var_span = Span::new(span.start + 3, span.end + 3);
    let idx_ident = || Expr::Ident { span: *span, name: idx_name.to_string() };
    let base_ident = || Expr::Ident { span: *span, name: base_name.to_string() };
    let (size_expr, get_expr) = match kind {
        AccessKind::Array => (
            Expr::Call {
                span: *span,
                callee: vec!["len".to_string()],
                type_args: Vec::new(),
                args: vec![base_ident()],
            },
            Expr::Index {
                span: *span,
                base: Box::new(base_ident()),
                idx: Box::new(idx_ident()),
            },
        ),
        AccessKind::Collection => (
            Expr::MethodCall {
                span: *span,
                base: Box::new(base_ident()),
                method: "size".to_string(),
                args: Vec::new(),
            },
            Expr::MethodCall {
                span: *span,
                base: Box::new(base_ident()),
                method: "get".to_string(),
                args: vec![idx_ident()],
            },
        ),
    };
    let cond = Expr::BinOp {
        span: *span,
        op: BinOp::Lt,
        l: Box::new(idx_ident()),
        r: Box::new(size_expr),
    };
    let update = Stmt::ExprStmt {
        span: *span,
        expr: Expr::IncrDecr {
            span: *span,
            e: Box::new(idx_ident()),
            inc: true,
            pre: false,
        },
    };
    let idx_decl = Stmt::Decl {
        span: idx_span,
        name: idx_name.to_string(),
        ty: Some(Ty::Int),
        value: Expr::Int { span: idx_span, value: 0 },
    };
    let var_decl = Stmt::Decl {
        span: var_span,
        name: name.clone(),
        ty: Some(elem_ty),
        value: get_expr,
    };
    let mut body_stmts = vec![var_decl];
    body_stmts.extend(body.stmts);
    let for_stmt = Stmt::For {
        span: *span,
        label,
        init: Some(Box::new(idx_decl)),
        cond: Some(Box::new(cond)),
        update: Some(Box::new(update)),
        body: Box::new(Block { span: *span, stmts: body_stmts }),
    };
    let base_decl = Stmt::Decl {
        span: base_span,
        name: base_name.to_string(),
        ty: Some(base_ty),
        value: *target,
    };
    Ok(vec![base_decl, for_stmt])
}

/// The `get(int)` return type and `size()` method of a class, walking the
/// parent chain (e.g. `size` lives on the `Collection` base of `List` /
/// `Queue` / `HashSet`).
fn find_accessors(prog: &Program, s: usize) -> (Option<Ty>, Option<Ty>) {
    let mut cur: Option<usize> = Some(s);
    let mut get: Option<Ty> = None;
    let mut size: Option<Ty> = None;
    let mut hops = 0;
    while let Some(c) = cur {
        hops += 1;
        if hops > 16 {
            break;
        }
        for m in &prog.structs[c].methods {
            if m.name == "get" && m.params.len() == 1 {
                if let Some(Ty::Int) = &m.params[0].1 {
                    get = m.ret.clone();
                }
            } else if m.name == "size" && m.params.is_empty() {
                size = m.ret.clone();
            }
        }
        cur = match &prog.structs[c].extends {
            Some(en) => prog.structs.iter().position(|x| x.name == *en).or(None),
            None => None,
        };
    }
    (get, size)
}

/// True for the std `HashMap` (the one collection with no element order
/// of its own; iterate `.keys()` or `.values()` instead).
fn is_map(prog: &Program, s: usize) -> bool {
    let mut cur: Option<usize> = Some(s);
    let mut hops = 0;
    while let Some(c) = cur {
        hops += 1;
        if hops > 16 {
            break;
        }
        let ms = &prog.structs[c].methods;
        if ms.iter().any(|m| m.name == "keys") && ms.iter().any(|m| m.name == "values") {
            return true;
        }
        cur = match &prog.structs[c].extends {
            Some(en) => prog.structs.iter().position(|x| x.name == *en).or(None),
            None => None,
        };
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    /// Array: `len` / `[]`.
    Array,
    /// Collection: `size()` / `get(i)`.
    Collection,
}
