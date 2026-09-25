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
        if args.len() != params.len() {
            return Err(CompileError::new(
                Span::new(0, 0),
                format!(
                    "function '{}' expects {} argument(s), got {}",
                    name,
                    params.len(),
                    args.len()
                ),
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
                out.push(self.expr_type(&args[i], subst)?);
            } else {
                out.push(self.resolve_type(pt, subst)?);
            }
        }
        Ok(out)
    }

    /// Best-effort type of an expression, for type-argument inference.
    pub(crate) fn expr_type(&mut self, e: &Expr, subst: &[(String, Ty)]) -> CompileResult<Ty> {
        match e {
            Expr::Int { .. } => Ok(Ty::Int),
            Expr::Bool { .. } => Ok(Ty::Bool),
            Expr::Str { .. } => Ok(Ty::Str),
            Expr::Null { .. } => Ok(Ty::Ptr),
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
            Expr::Call { callee, .. } => {
                if let Some(fi) = self.resolve_func(callee) {
                    let (is_generic, ret, is_async) = {
                        let f = &self.prog.funcs[fi];
                        (
                            f.type_params.is_empty(),
                            f.ret.clone(),
                            f.is_async,
                        )
                    };
                    if is_async {
                        // An async call evaluates to a std.Task handle.
                        if let Some(ti) = self.class_by_name.get("std.Task") {
                            return Ok(Ty::Struct(self.struct_new[ti]));
                        }
                    }
                    if !is_generic {
                        if let Some(r) = ret {
                            return Ok(self.resolve_type(&r, subst)?);
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
            _ => Err(CompileError::new(
                Span::new(0, 0),
                "cannot infer type; specify type arguments explicitly",
            )),
        }
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
