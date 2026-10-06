use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use super::Mono;

impl<'a> Mono<'a> {
    /// Infer the type arguments of a generic call from its argument types.
    pub(crate) fn infer_func_args(
        &mut self,
        fi: usize,
        args: &[Expr],
        subst: &[(String, Ty)],
    ) -> CompileResult<Vec<Ty>> {
        let (name, params) = {
            let f = &self.prog.funcs[fi];
            (f.name.clone(), f.params.clone())
        };
        // Omitted trailing arguments fall back to their default values.
        let min_req = params.iter().filter(|p| p.3.is_none()).count();
        if args.len() < min_req || args.len() > params.len() {
            return Err(CompileError::new(
                Span::new(0, 0),
                super::arity_msg(&name, min_req, params.len(), args.len()),
            ));
        }
        let mut out = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let pt = match &p.1 {
                Some(t) => t,
                None => {
                    out.push(Ty::Int);
                    continue;
                }
            };
            if matches!(pt, Ty::Param(_)) {
                let src = if i < args.len() { &args[i] } else { p.3.as_ref().unwrap() };
                out.push(self.expr_type(src, subst)?);
            } else {
                out.push(self.resolve_type(pt, subst)?);
            }
        }
        // One entry per type parameter (declaration order), inferred from
        // the first parameter of that type. A type parameter that no
        // parameter uses falls back to int.
        let type_params = self.prog.funcs[fi].type_params.clone();
        let mut res = Vec::new();
        for name in &type_params {
            let idx = params
                .iter()
                .position(|p| matches!(&p.1, Some(Ty::Param(n)) if n == name));
            res.push(match idx {
                Some(i) => out[i].clone(),
                None => Ty::Int,
            });
        }
        Ok(res)
    }

    /// Best-effort type of an expression, for type-argument inference and
    /// `var` declarations.
    pub(crate) fn expr_type(&mut self, e: &Expr, subst: &[(String, Ty)]) -> CompileResult<Ty> {
        match e {
            Expr::Int { .. } => Ok(Ty::Int),
            Expr::Bool { .. } => Ok(Ty::Bool),
            Expr::Str { .. } => Ok(Ty::Str),
            Expr::Interp { .. } => Ok(Ty::Str),
            Expr::Null { .. } => Ok(Ty::Ptr(None)),
            Expr::EnumVariant { .. } => Ok(Ty::Int),
            Expr::This { .. } => self
                .cur_this
                .map(Ty::Struct)
                .ok_or_else(|| CompileError::new(Span::new(0, 0), "cannot infer type of 'this'")),
            Expr::SuperBase { .. } => self
                .cur_this
                .map(Ty::Struct)
                .ok_or_else(|| CompileError::new(Span::new(0, 0), "cannot infer type of 'super'")),
            Expr::Ident { name, .. } => self
                .cur_locals
                .get(name)
                .cloned()
                .ok_or_else(|| {
                    CompileError::new(
                        Span::new(0, 0),
                        format!(
                            "cannot infer type of '{}'; specify type arguments explicitly",
                            name
                        ),
                    )
                }),
            Expr::BinOp { op, l, .. } => {
                let lt = self.expr_type(l, subst)?;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Le
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Ok(Ty::Bool),
                    _ => Ok(lt),
                }
            }
            Expr::UnOp { op, e: inner, .. } => match op {
                UnOp::Neg => Ok(Ty::Int),
                UnOp::Not => Ok(Ty::Bool),
                UnOp::BitNot => {
                    self.expr_type(inner, subst)?;
                    Ok(Ty::Int)
                }
                UnOp::Addr => Ok(Ty::Ptr(None)),
                UnOp::FnAddr => Ok(Ty::Ptr(None)),
                UnOp::Deref => {
                    let t = self.expr_type(inner, subst)?;
                    match t {
                        Ty::Ptr(Some(p)) => Ok(*p),
                        _ => Ok(Ty::Int),
                    }
                }
            },
            Expr::IncrDecr { e: inner, .. } => self.expr_type(inner, subst),
            Expr::AddrOf { e: inner, .. } => {
                self.expr_type(inner, subst)?;
                Ok(Ty::Ptr(None))
            }
            Expr::FnAddr { e: inner, .. } => {
                self.expr_type(inner, subst)?;
                Ok(Ty::Ptr(None))
            }
            Expr::Deref { e: inner, .. } => {
                let t = self.expr_type(inner, subst)?;
                match t {
                    Ty::Ptr(Some(p)) => Ok(*p),
                    _ => Ok(Ty::Int),
                }
            }
            Expr::Index { base, .. } => {
                let bt = self.expr_type(base, subst)?;
                if matches!(bt, Ty::Array) {
                    Ok(Ty::Array)
                } else {
                    Ok(Ty::Int)
                }
            }
            Expr::ArrayLit { .. } => Ok(Ty::Array),
            Expr::SizedNew { ty, .. } => Ok(ty.clone()),
            Expr::Cast { ty, .. } => self.resolve_type(ty, subst),
            Expr::Instanceof { .. } => Ok(Ty::Bool),
            Expr::Cond { then, .. } => self.expr_type(then, subst),
            Expr::OptChain { base, rest, .. } => {
                let bt = self.expr_type(base, subst)?;
                let saved = std::mem::replace(&mut self.opt_ref_ty, Some(bt));
                let r = self.expr_type(rest, subst);
                self.opt_ref_ty = saved;
                r
            }
            Expr::OptRef { span } => self
                .opt_ref_ty
                .clone()
                .ok_or_else(|| CompileError::new(*span, "cannot infer type of optional-chain base")),
            Expr::Coalesce { l, .. } => self.expr_type(l, subst),
            Expr::Closure { .. } => Ok(Ty::Ptr(None)),
            Expr::Lambda { .. } => Ok(Ty::Ptr(None)),
            Expr::StructLit {
                name,
                type_args,
                span,
                ..
            } => {
                // A resolved `new` carries the mangled instance name (its
                // type args were cleared during resolution): map it back
                // to the instance index.
                if let Some((ni, _)) = self
                    .class_names
                    .iter()
                    .find(|(_, n)| **n == *name)
                {
                    return Ok(Ty::Struct(*ni));
                }
                let ci = *self
                    .class_by_name
                    .get(name)
                    .ok_or_else(|| {
                        CompileError::new(*span, format!("unknown class '{}'", name))
                    })?;
                let resolved: Vec<Ty> = type_args
                    .iter()
                    .map(|a| self.resolve_type(a, subst))
                    .collect::<CompileResult<Vec<Ty>>>()?;
                let ni = self.ensure_class(ci, resolved)?;
                Ok(Ty::Struct(ni))
            }
            Expr::Call { callee, type_args, .. } => {
                if let Some(fi) = self.resolve_func(callee) {
                    let (is_generic, ret, is_async, type_params) = {
                        let f = &self.prog.funcs[fi];
                        (
                            !f.type_params.is_empty(),
                            f.ret.clone(),
                            f.is_async,
                            f.type_params.clone(),
                        )
                    };
                    if is_async {
                        // An async call evaluates to a std.Task handle.
                        if let Some(ti) = self.class_by_name.get("std.Task") {
                            return Ok(Ty::Struct(self.struct_new[ti]));
                        }
                    }
                    if let Some(r) = &ret {
                        if is_generic && !type_args.is_empty() {
                            // Explicit type arguments: substitute them for the
                            // type parameters in the return type.
                            let gsubst: Vec<(String, Ty)> = type_params
                                .iter()
                                .zip(type_args.iter())
                                .map(|(p, a)| (p.clone(), a.clone()))
                                .collect();
                            return self.resolve_type(r, &gsubst).or_else(|_| {
                                self.resolve_type(r, subst)
                            });
                        }
                        if !is_generic || !type_params.is_empty() {
                            return self.resolve_type(r, subst);
                        }
                    }
                }
                Err(CompileError::new(
                    Span::new(0, 0),
                    "cannot infer type of call; specify type arguments explicitly",
                ))
            }
            Expr::Await { e: inner, .. } => {
                // `await f(args)` has the async `f`'s return type (0 for
                // void); `await t` for a `Task t` is an int (the worker's
                // value). Best effort: a plain Task-typed local is an int.
                if let Expr::Call { callee, .. } = inner.as_ref() {
                    if let Some(fi) = self.resolve_func(callee) {
                        let (is_async, ret) = {
                            let f = &self.prog.funcs[fi];
                            (f.is_async, f.ret.clone())
                        };
                        if is_async {
                            return Ok(await_type(ret));
                        }
                    }
                    return Err(CompileError::new(
                        Span::new(0, 0),
                        "await needs an async call; specify type arguments explicitly",
                    ));
                }
                if let Expr::MethodCall { base, method, .. } = inner.as_ref() {
                    // Best effort: the base is a local of a known class type;
                    // walk its parent chain for the method.
                    if let Expr::Ident { name, .. } = base.as_ref() {
                        if let Some(Ty::Struct(s)) = self.cur_locals.get(name).cloned() {
                            let mut si = s;
                            for _ in 0..16 {
                                if let Some(m) = self.out.structs[si]
                                    .methods
                                    .iter()
                                    .find(|m| m.name == *method)
                                {
                                    if m.is_async {
                                        return Ok(await_type(m.ret.clone()));
                                    }
                                    break;
                                }
                                match self.out.structs[si].extends.clone() {
                                    Some(en) => match self
                                        .out
                                        .structs
                                        .iter()
                                        .position(|x| x.name == en)
                                    {
                                        Some(n) => si = n,
                                        None => break,
                                    },
                                    None => break,
                                }
                            }
                        }
                    }
                    return Err(CompileError::new(
                        Span::new(0, 0),
                        "await needs an async method; specify type arguments explicitly",
                    ));
                }
                if let Expr::Ident { name, .. } = inner.as_ref() {
                    if let Some(Ty::Struct(s)) = self.cur_locals.get(name).cloned() {
                        if self.class_names.get(&s).map(|n| n.as_str()) == Some("std_Task") {
                            return Ok(Ty::Int);
                        }
                    }
                }
                Err(CompileError::new(
                    Span::new(0, 0),
                    "cannot infer type of await; specify type arguments explicitly",
                ))
            }
            Expr::Field { base, name, span } => {
                let bt = self.expr_type(base, subst)?;
                let s = match bt {
                    Ty::Struct(s) => s,
                    _ => {
                        return Err(CompileError::new(
                            *span,
                            "cannot infer type of field access; specify type arguments explicitly",
                        ))
                    }
                };
                self.struct_field_type(s, name).ok_or_else(|| {
                    CompileError::new(
                        *span,
                        format!(
                            "cannot infer type of '{}'; specify type arguments explicitly",
                            name
                        ),
                    )
                })
            }
            Expr::MethodCall { base, method, .. } => {
                // A dotted-path base whose root is not a local is a qualified
                // free-function call (e.g. `com.example.other()`).
                if let Some(mut path) = dotted_path(base.as_ref()) {
                    path.push(method.clone());
                    if path.len() >= 2
                        && !self.cur_locals.contains_key(&path[0])
                        && self.resolve_func(&path).is_some()
                    {
                        // Delegate to the call inference (returns the function's
                        // ret; an unresolved ret falls through to the error).
                        return self.expr_type(
                            &Expr::Call {
                                span: Span::new(0, 0),
                                callee: path,
                                type_args: Vec::new(),
                                args: Vec::new(),
                            },
                            subst,
                        );
                    }
                }
                let bt = self.expr_type(base, subst)?;
                let s = match bt {
                    Ty::Struct(s) => s,
                    _ => {
                        return Err(CompileError::new(
                            Span::new(0, 0),
                            "cannot infer type of method call; specify type arguments explicitly",
                        ))
                    }
                };
                // Walk the parent chain (a `List.get` lives on `List`, but
                // `size` lives on the `Collection` base).
                let mut cur: Option<usize> = Some(s);
                let mut hops = 0;
                while let Some(c) = cur {
                    hops += 1;
                    if hops > 16 {
                        break;
                    }
                    let cls = &self.out.structs[c];
                    if let Some(m) = cls.methods.iter().find(|m| m.name == *method) {
                        return Ok(m.ret.clone().unwrap_or(Ty::Int));
                    }
                    // A get/set accessor: the field's type.
                    for f in &cls.fields {
                        if f.accessor == Accessor::Get || f.accessor == Accessor::GetSet {
                            if getter_name(&f.name) == *method {
                                return Ok(f.ty.clone());
                            }
                        }
                    }
                    cur = match &cls.extends {
                        Some(en) => self
                            .out
                            .structs
                            .iter()
                            .position(|x| x.name == *en)
                            .or(None),
                        None => None,
                    };
                }
                Err(CompileError::new(
                    Span::new(0, 0),
                    format!(
                        "cannot infer type of method '{}'; specify type arguments explicitly",
                        method
                    ),
                ))
            }
            Expr::SuperCall { .. } => Ok(Ty::Void),
        }
    }

    /// The type of a field of a (monomorphized) class, walking the parent
    /// chain for inherited fields.
    fn struct_field_type(&self, s: usize, name: &str) -> Option<Ty> {
        let mut cur: Option<usize> = Some(s);
        let mut hops = 0;
        while let Some(c) = cur {
            hops += 1;
            if hops > 16 {
                return None;
            }
            let cls = &self.out.structs[c];
            if let Some(f) = cls.fields.iter().find(|f| &f.name == name) {
                return Some(f.ty.clone());
            }
            cur = match &cls.extends {
                Some(en) => self
                    .out
                    .structs
                    .iter()
                    .position(|x| x.name == *en)
                    .or(None),
                None => None,
            };
        }
        None
    }
}

/// Extract a dotted path (root identifier + field names) from a pure
/// identifier/field chain.
fn dotted_path(e: &Expr) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut cur = e;
    loop {
        match cur {
            Expr::Ident { name, .. } => {
                names.insert(0, name.clone());
                break;
            }
            Expr::Field { base, name, .. } => {
                names.insert(0, name.clone());
                cur = base;
            }
            _ => return None,
        }
    }
    Some(names)
}

/// `get` + PascalCase(field), matching the backend's accessor naming.
fn getter_name(field: &str) -> String {
    let trimmed = field.trim_start_matches('_');
    if trimmed.is_empty() {
        return "get".to_string();
    }
    format!(
        "get{}",
        trimmed
            .split('_')
            .filter(|part| !part.is_empty())
            .map(capitalize)
            .collect::<String>()
    )
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The type of the value `await` yields for an async definition: the
/// declared return type, or `int` (0) for a void function.
fn await_type(ret: Option<Ty>) -> Ty {
    match ret {
        Some(Ty::Void) | None => Ty::Int,
        Some(r) => r,
    }
}
