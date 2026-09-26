use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use super::Mono;

/// Extract a dotted path (root identifier + field names) from a pure
/// identifier/field chain. Returns None if the expression contains anything
/// else (calls, literals, this/super, ...).
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

impl<'a> Mono<'a> {
    pub(crate) fn resolve_expr(&mut self, e: &Expr, subst: &[(String, Ty)]) -> CompileResult<Expr> {
        match e {
            Expr::Int { .. }
            | Expr::Bool { .. }
            | Expr::Str { .. }
            | Expr::Null { .. }
            | Expr::This { .. }
            | Expr::Ident { .. } => Ok(e.clone()),
            Expr::Closure {
                span,
                fn_name,
                captures,
            } => {
                // Lambdas are desugared before monomorphization: the
                // generated function is non-generic (it carries the
                // caller's concrete types), so `fn_name` is left untouched.
                let captures: Vec<Expr> = captures
                    .iter()
                    .map(|c| self.resolve_expr(c, subst))
                    .collect::<CompileResult<Vec<Expr>>>()?;
                Ok(Expr::Closure {
                    span: *span,
                    fn_name: fn_name.clone(),
                    captures,
                })
            }
            Expr::Lambda {
                span,
                params: lparams,
                ret,
                body,
            } => {
                // A surviving lambda is a diagnostic path (it should have
                // been lifted); resolve it defensively.
                let params = lparams
                    .iter()
                    .map(|(p, t, s2)| {
                        let t = t
                            .as_ref()
                            .map(|t2| self.resolve_type(t2, subst))
                            .transpose()?;
                        Ok((p.clone(), t, *s2))
                    })
                    .collect::<CompileResult<Vec<(String, Option<Ty>, Span)>>>()?;
                Ok(Expr::Lambda {
                    span: *span,
                    params,
                    ret: ret
                        .as_ref()
                        .map(|t| self.resolve_type(t, subst))
                        .transpose()?,
                    body: self.resolve_block(body, subst)?,
                })
            }
            Expr::EnumVariant {
                span, enum_name, variant, ..
            } => {
                // Look up the variant value from the enum definition.
                let eidx = self
                    .prog
                    .enums
                    .iter()
                    .position(|e| e.name == *enum_name)
                    .ok_or_else(|| {
                        CompileError::new(*span, format!("unknown enum '{}'", enum_name))
                    })?;
                let val = self.prog.enums[eidx]
                    .variants
                    .iter()
                    .find(|(vn, _)| vn == variant)
                    .map(|(_, v)| *v)
                    .ok_or_else(|| {
                        CompileError::new(*span, format!("unknown variant '{}.{}'", enum_name, variant))
                    })?;
                Ok(Expr::Int { span: *span, value: val })
            }
            Expr::Call {
                callee,
                type_args,
                args,
                ..
            } => {
                let args: Vec<Expr> = args
                    .iter()
                    .map(|a| self.resolve_expr(a, subst))
                    .collect::<CompileResult<_>>()?;
                if let Some(fi) = self.resolve_func(callee) {
                    let is_generic = !self.prog.funcs[fi].type_params.is_empty();
                    let ni = if is_generic {
                        let raw: Vec<Ty> = if type_args.is_empty() {
                            self.infer_func_args(fi, &args, subst)?
                        } else {
                            type_args
                                .iter()
                                .map(|a| self.resolve_type(a, subst))
                                .collect::<CompileResult<_>>()?
                        };
                        self.ensure_func(fi, raw)?
                    } else {
                        // Non-generic free function: rewrite the callee to the
                        // mangled (package-prefixed) symbol name so the backend
                        // can find it.
                        self.func_new[&fi]
                    };
                    let mut out = e.clone();
                    if let Expr::Call {
                        callee: c,
                        type_args: ta,
                        args: a,
                        ..
                    } = &mut out
                    {
                        *c = vec![self.func_names[&ni].clone()];
                        *ta = Vec::new();
                        *a = args;
                    }
                    return Ok(out);
                }
                let mut out = e.clone();
                if let Expr::Call { args: a, .. } = &mut out {
                    *a = args;
                }
                Ok(out)
            }
            Expr::MethodCall {
                span,
                base,
                method,
                args,
            } => {
                // A dotted-path base may be a qualified free-function call
                // (`com.example.other()`) rather than a method call. The
                // root must not be a local (else it is a real method call
                // on a value, e.g. `g.hello()`).
                if let Some(mut path) = dotted_path(base.as_ref()) {
                    path.push(method.clone());
                    if path.len() >= 2 && !self.cur_locals.contains_key(&path[0]) {
                        if self.resolve_func(&path).is_some() {
                            let call = Expr::Call {
                                span: *span,
                                callee: path,
                                type_args: Vec::new(),
                                args: args.clone(),
                            };
                            return self.resolve_expr(&call, subst);
                        }
                    }
                }
                let base = Box::new(self.resolve_expr(base, subst)?);
                let args: Vec<Expr> = args
                    .iter()
                    .map(|a| self.resolve_expr(a, subst))
                    .collect::<CompileResult<_>>()?;
                Ok(Expr::MethodCall {
                    span: *span,
                    base,
                    method: method.clone(),
                    args,
                })
            }
            Expr::BinOp { span, op, l, r } => {
                let l = Box::new(self.resolve_expr(l, subst)?);
                let r = Box::new(self.resolve_expr(r, subst)?);
                Ok(Expr::BinOp {
                    span: *span,
                    op: *op,
                    l,
                    r,
                })
            }
            Expr::UnOp { span, op, e: inner } => {
                let inner = Box::new(self.resolve_expr(inner, subst)?);
                Ok(Expr::UnOp {
                    span: *span,
                    op: *op,
                    e: inner,
                })
            }
            Expr::IncrDecr {
                span,
                e: inner,
                inc,
                pre,
            } => {
                let inner = Box::new(self.resolve_expr(inner, subst)?);
                Ok(Expr::IncrDecr {
                    span: *span,
                    e: inner,
                    inc: *inc,
                    pre: *pre,
                })
            }
            Expr::AddrOf { span, e: inner } => {
                let inner = Box::new(self.resolve_expr(inner, subst)?);
                Ok(Expr::AddrOf {
                    span: *span,
                    e: inner,
                })
            }
            Expr::Deref { span, e: inner } => {
                let inner = Box::new(self.resolve_expr(inner, subst)?);
                Ok(Expr::Deref {
                    span: *span,
                    e: inner,
                })
            }
            Expr::FnAddr { span, e: inner } => {
                let inner = Box::new(self.resolve_expr(inner, subst)?);
                Ok(Expr::FnAddr {
                    span: *span,
                    e: inner,
                })
            }
            Expr::Index { span, base, idx } => {
                let base = Box::new(self.resolve_expr(base, subst)?);
                let idx = Box::new(self.resolve_expr(idx, subst)?);
                Ok(Expr::Index {
                    span: *span,
                    base,
                    idx,
                })
            }
            Expr::Field { span, base, name } => {
                let base = Box::new(self.resolve_expr(base, subst)?);
                Ok(Expr::Field {
                    span: *span,
                    base,
                    name: name.clone(),
                })
            }
            Expr::StructLit {
                span,
                name,
                type_args,
                fields,
            } => {
                let ci = match self.class_by_name.get(name) {
                    Some(&ci) => ci,
                    None => {
                        return Err(CompileError::new(
                            *span,
                            format!("unknown class '{}'", name),
                        ))
                    }
                };
                let resolved_args: Vec<Ty> = type_args
                    .iter()
                    .map(|a| self.resolve_type(a, subst))
                    .collect::<CompileResult<_>>()?;
                let ni = self.ensure_class(ci, resolved_args)?;
                let fields: Vec<(String, Expr)> = fields
                    .iter()
                    .map(|(n, f)| {
                        let f = self.resolve_expr(f, subst)?;
                        Ok((n.clone(), f))
                    })
                    .collect::<CompileResult<_>>()?;
                Ok(Expr::StructLit {
                    span: *span,
                    name: self.class_names[&ni].clone(),
                    type_args: Vec::new(),
                    fields,
                })
            }
            Expr::ArrayLit { span, elems } => {
                let elems: Vec<Expr> = elems
                    .iter()
                    .map(|a| self.resolve_expr(a, subst))
                    .collect::<CompileResult<_>>()?;
                Ok(Expr::ArrayLit { span: *span, elems })
            }
            Expr::Cond {
                span,
                cond,
                then,
                els,
            } => {
                let cond = Box::new(self.resolve_expr(cond, subst)?);
                let then = Box::new(self.resolve_expr(then, subst)?);
                let els = Box::new(self.resolve_expr(els, subst)?);
                Ok(Expr::Cond {
                    span: *span,
                    cond,
                    then,
                    els,
                })
            }
            Expr::Await { span, e: inner } => {
                let inner = Box::new(self.resolve_expr(inner, subst)?);
                Ok(Expr::Await {
                    span: *span,
                    e: inner,
                })
            }
            Expr::SuperBase { span } => Ok(Expr::SuperBase { span: *span }),
            Expr::SuperCall { span, args } => {
                let args = args
                    .iter()
                    .map(|a| self.resolve_expr(a, subst))
                    .collect::<CompileResult<_>>()?;
                Ok(Expr::SuperCall { span: *span, args })
            }
            Expr::Cast { span, ty, e } => {
                let ty = self.resolve_type(ty, subst)?;
                let e = Box::new(self.resolve_expr(e, subst)?);
                Ok(Expr::Cast { span: *span, ty, e })
            }
            Expr::Instanceof { span, e, ty } => {
                let e = Box::new(self.resolve_expr(e, subst)?);
                let ty = self.resolve_type(ty, subst)?;
                Ok(Expr::Instanceof { span: *span, e, ty })
            }
        }
    }
}
