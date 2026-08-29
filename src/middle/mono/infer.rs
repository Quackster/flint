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
                    let (is_generic, ret) = {
                        let f = &self.prog.funcs[fi];
                        (f.type_params.is_empty(), f.ret.clone())
                    };
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
            _ => Err(CompileError::new(
                Span::new(0, 0),
                "cannot infer type; specify type arguments explicitly",
            )),
        }
    }
}
