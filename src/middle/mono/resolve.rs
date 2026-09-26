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
        let mut out = block.clone();
        for s in out.stmts.iter_mut() {
            *s = self.resolve_stmt(s, subst)?;
        }
        Ok(out)
    }

    pub(crate) fn resolve_stmt(&mut self, stmt: &Stmt, subst: &[(String, Ty)]) -> CompileResult<Stmt> {
        match stmt {
            Stmt::Decl { span, name, ty, value } => {
                let value = self.resolve_expr(value, subst)?;
                let ty = match ty {
                    Some(t) => Some(self.resolve_type(t, subst)?),
                    None => None,
                };
                let lty = ty
                    .clone()
                    .or_else(|| self.expr_type(&value, subst).ok());
                if let Some(t) = &lty {
                    self.cur_locals.insert(name.clone(), t.clone());
                }
                Ok(Stmt::Decl {
                    span: *span,
                    name: name.clone(),
                    ty,
                    value,
                })
            }
            Stmt::Assign { span, target, value } => {
                let target = self.resolve_expr(target, subst)?;
                let value = self.resolve_expr(value, subst)?;
                Ok(Stmt::Assign {
                    span: *span,
                    target,
                    value,
                })
            }
            Stmt::ExprStmt { span, expr } => {
                let expr = self.resolve_expr(expr, subst)?;
                Ok(Stmt::ExprStmt { span: *span, expr })
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
                Ok(Stmt::If {
                    span: *span,
                    cond,
                    then,
                    else_opt,
                })
            }
            Stmt::While { span, label, cond, body } => {
                let cond = Box::new(self.resolve_expr(cond, subst)?);
                let body = Box::new(self.resolve_block(body, subst)?);
                Ok(Stmt::While {
                    span: *span,
                    label: label.clone(),
                    cond,
                    body,
                })
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
                    Some(s) => Some(Box::new(self.resolve_stmt(s, subst)?)),
                    None => None,
                };
                let cond = match cond {
                    Some(c) => Some(Box::new(self.resolve_expr(c, subst)?)),
                    None => None,
                };
                let update = match update {
                    Some(s) => Some(Box::new(self.resolve_stmt(s, subst)?)),
                    None => None,
                };
                let body = Box::new(self.resolve_block(body, subst)?);
                Ok(Stmt::For {
                    span: *span,
                    label: label.clone(),
                    init,
                    cond,
                    update,
                    body,
                })
            }
            Stmt::Return { span, value } => {
                let value = match value {
                    Some(v) => Some(Box::new(self.resolve_expr(v, subst)?)),
                    None => None,
                };
                Ok(Stmt::Return { span: *span, value })
            }
            Stmt::Break { span, label } => Ok(Stmt::Break { span: *span, label: label.clone() }),
            Stmt::Continue { span, label } => Ok(Stmt::Continue { span: *span, label: label.clone() }),
            Stmt::CompoundAssign {
                span,
                target,
                op,
                value,
            } => {
                let target = self.resolve_expr(target, subst)?;
                let value = self.resolve_expr(value, subst)?;
                Ok(Stmt::CompoundAssign {
                    span: *span,
                    target,
                    op: *op,
                    value,
                })
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
                Ok(Stmt::Switch {
                    span: *span,
                    target,
                    cases,
                    default,
                })
            }
            Stmt::Throw { span, value } => {
                let value = Box::new(self.resolve_expr(value, subst)?);
                Ok(Stmt::Throw { span: *span, value })
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
                Ok(Stmt::TryCatch {
                    span: *span,
                    try_block,
                    catch_type: catch_type.clone(),
                    catch_var: catch_var.clone(),
                    catch_block,
                    finally,
                })
            }
        }
    }
}
