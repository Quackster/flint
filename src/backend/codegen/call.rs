use crate::ast::{Accessor, Expr, Ty};
use crate::error::{CompileError, CompileResult};
use crate::span::Span;
use crate::backend::layout;

use super::builtin::builtin_for;
use super::{ARGREGS, Ctx, Frame, getter_name, mangle, setter_name, ty_name};

impl Ctx<'_> {
    pub(crate) fn gen_call(
        &mut self,
        callee: &[String],
        args: &[Expr],
        frame: &mut Frame,
        span: Span,
    ) -> CompileResult<Ty> {
        if let Some(b) = builtin_for(callee) {
            if args.len() != b.arity {
                return Err(CompileError::new(
                    span,
                    format!("{} expects {} arguments, got {}", callee.join("."), b.arity, args.len()),
                ));
            }
            // len() only makes sense on arrays and collections (ident args
            // are checked; other expressions fall through untyped in v1)
            if b.target == "flint_len" {
                if let Expr::Ident { name, span: aspan } = &args[0] {
                    if let Some(l) = frame.find(name) {
                        if l.ty != Ty::Array && l.ty.coll_kind().is_none() {
                            return Err(CompileError::new(
                                *aspan,
                                format!("len() requires an array or collection, got a {}", ty_name(&l.ty)),
                            ));
                        }
                    }
                }
            }
            // builtins read their arguments; literals stay read-only
            for a in args {
                self.gen_expr_ro(a, frame)?;
            }
            self.pop_args(args.len());
            self.emit(&format!("\tcall {}", b.target));
            if b.noreturn {
                // control does not return; push a placeholder so callers stay balanced
                self.emit("\tmovq $0, %rax");
                self.emit("\tpush %rax");
            } else if b.ret == Ty::Void {
                self.emit("\tmovq $0, %rax");
                self.emit("\tpush %rax");
            } else {
                self.emit("\tpush %rax");
            }
            return Ok(b.ret);
        }
        // user function
        if callee.len() != 1 {
            return Err(CompileError::new(
                span,
                format!("unknown function '{}'", callee.join(".")),
            ));
        }
        let name = &callee[0];
        let idx = match self.func_idx.get(name) {
            Some(i) => *i,
            None => {
                return Err(CompileError::new(
                    span,
                    format!("undefined function '{}'", name),
                ))
            }
        };
        let f = &self.prog.funcs[idx];
        if args.len() != f.params.len() {
            return Err(CompileError::new(
                span,
                format!(
                    "'{}' expects {} arguments, got {}",
                    name,
                    f.params.len(),
                    args.len()
                ),
            ));
        }
        // The callee owns the argument references it receives: retain copies
        // of existing references (fresh values transfer as-is). String
        // literals for `string` params get a fresh writable copy (the
        // callee may write through its parameters).
        let saved = self.str_copy;
        for (i, a) in args.iter().enumerate() {
            self.str_copy = f.params[i].1.as_ref() == Some(&Ty::Str);
            self.gen_expr(a, frame)?;
            self.maybe_retain(a, frame);
        }
        self.str_copy = saved;
        self.pop_args(args.len());
        self.emit(&format!("\tcall {}", name));
        if f.ret == Some(Ty::Void) || f.ret.is_none() {
            self.emit("\tmovq $0, %rax");
            self.emit("\tpush %rax");
        } else {
            self.emit("\tpush %rax");
        }
        Ok(f.ret.clone().unwrap_or(Ty::Int))
    }

    pub(crate) fn gen_method_call(
        &mut self,
        base: &Expr,
        method: &str,
        args: &[Expr],
        frame: &mut Frame,
        span: Span,
    ) -> CompileResult<Ty> {
        // built-in collections dispatch before class methods
        if let Some(cty) = self.coll_type_of(base, frame)? {
            return self.gen_coll_method(base, cty, method, args, frame, span);
        }
        // interface-typed base: dispatch through the fixed interface slot
        if let Some(i) = self.expr_interface_type(base, frame)? {
            return self.gen_iface_method_call(base, i, method, args, frame, span);
        }
        // `ClassName.method(args)`: a class name as the base denotes a
        // static call (e.g. `Math.abs(7)`)
        let (sidx, via_class_name) = match self.expr_struct_type(base, frame)? {
            Some(i) => (i, false),
            None => match base {
                Expr::Ident { name, .. } => match self.struct_idx_by_name(name) {
                    Some(ci) => (ci, true),
                    None => {
                        return Err(CompileError::new(span, "method call requires a class-typed base"))
                    }
                },
                _ => {
                    return Err(CompileError::new(span, "method call requires a class-typed base"))
                }
            },
        };
        let class_def = &self.prog.structs[sidx];
        // try real method first (own or inherited), then synthetic accessor
        let found = layout::find_method_def(self.prog, sidx, method);
        if let Some((def_ci, mi)) = found {
            let meth = &self.prog.structs[def_ci].methods[mi];
            self.check_method_access(sidx, method, frame, span)?;
            if !meth.is_static && via_class_name {
                return Err(CompileError::new(
                    span,
                    format!(
                        "method '{}' of class '{}' is not static; call it on an instance",
                        method, class_def.name
                    ),
                ));
            }
            if meth.is_static {
                if args.len() != meth.params.len() {
                    return Err(CompileError::new(
                        span,
                        format!(
                            "static method '{}' expects {} args, got {}",
                            method,
                            meth.params.len(),
                            args.len()
                        ),
                    ));
                }
                // The callee owns the argument references it receives.
                let saved = self.str_copy;
                for (i, a) in args.iter().enumerate() {
                    self.str_copy = meth.params[i].1.as_ref() == Some(&Ty::Str);
                    self.gen_expr(a, frame)?;
                    self.maybe_retain(a, frame);
                }
                self.str_copy = saved;
                self.pop_args(args.len());
                let mangled = mangle(&class_def.name, method);
                self.emit(&format!("\tcall {}", mangled));
                if meth.ret == Some(Ty::Void) {
                    self.emit("\tmovq $0, %rax");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Void);
                } else {
                    self.emit("\tpush %rax");
                    return Ok(meth.ret.clone().unwrap_or(Ty::Int));
                }
            } else {
                if args.len() != meth.params.len() {
                    return Err(CompileError::new(
                        span,
                        format!(
                            "method '{}' expects {} args, got {}",
                            method,
                            meth.params.len(),
                            args.len()
                        ),
                    ));
                }
                // The callee owns the base and argument references it receives.
                self.gen_expr_ro(base, frame)?;
                self.maybe_retain(base, frame);
                let saved = self.str_copy;
                for (i, a) in args.iter().enumerate() {
                    self.str_copy = meth.params[i].1.as_ref() == Some(&Ty::Str);
                    self.gen_expr(a, frame)?;
                    self.maybe_retain(a, frame);
                }
                self.str_copy = saved;
                let total = args.len() + 1;
                if total > 6 {
                    return Err(CompileError::new(
                        span,
                        "too many arguments for method (including this)",
                    ));
                }
                self.pop_args(total);
                // vtable dispatch when the static type has a vtable slot and
                // this is not a super call (super calls the parent's version
                // statically, bypassing the vtable)
                let is_super = matches!(base, Expr::SuperBase { .. });
                if !is_super && layout::has_vtable_slot(self.prog, sidx) {
                    let slot = layout::vtable_slots(self.prog, sidx)
                        .iter()
                        .position(|n| n == method)
                        .ok_or_else(|| {
                            CompileError::new(span, format!("method '{}' not in vtable", method))
                        })?;
                    self.emit("\tmovq 8(%rdi), %r11");
                    self.emit(&format!("\tmovq {}*8(%r11), %r12", slot));
                    self.emit("\tcall *%r12");
                } else {
                    let def_name = self.prog.structs[def_ci].name.clone();
                    let mangled = mangle(&def_name, method);
                    self.emit(&format!("\tcall {}", mangled));
                }
                if meth.ret == Some(Ty::Void) {
                    self.emit("\tmovq $0, %rax");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Void);
                } else {
                    self.emit("\tpush %rax");
                    return Ok(meth.ret.clone().unwrap_or(Ty::Int));
                }
            }
        }
        if via_class_name {
            return Err(CompileError::new(
                span,
                format!("class '{}' has no static method '{}'", class_def.name, method),
            ));
        }
        // check synthetic getter/setter derived from fields
        for f in &class_def.fields {
            if f.accessor == Accessor::Get || f.accessor == Accessor::GetSet {
                let gname = getter_name(&f.name);
                if gname == method {
                    if !args.is_empty() {
                        return Err(CompileError::new(
                            span,
                            format!("getter '{}' expects 0 args, got {}", method, args.len()),
                        ));
                    }
                    // synthesize getter: no privacy check (consider public)
                    self.gen_expr(base, frame)?;
                    self.maybe_retain(base, frame);
                    self.pop_args(1);
                    let mangled = mangle(&class_def.name, &gname);
                    self.emit(&format!("\tcall {}", mangled));
                    self.emit("\tpush %rax");
                    return Ok(f.ty.clone());
                }
            }
            if f.accessor == Accessor::Set || f.accessor == Accessor::GetSet {
                let sname = setter_name(&f.name);
                if sname == method {
                    if args.len() != 1 {
                        return Err(CompileError::new(
                            span,
                            format!("setter '{}' expects 1 arg, got {}", method, args.len()),
                        ));
                    }
                    // setter: this + value (callee owns both references)
                    self.gen_expr(base, frame)?;
                    self.maybe_retain(base, frame);
                    self.gen_expr(&args[0], frame)?;
                    self.maybe_retain(&args[0], frame);
                    self.pop_args(2);
                    let mangled = mangle(&class_def.name, &sname);
                    self.emit(&format!("\tcall {}", mangled));
                    self.emit("\tmovq $0, %rax");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Void);
                }
            }
        }
        return Err(CompileError::new(
            span,
            format!("class '{}' has no method '{}'", class_def.name, method),
        ));
    }

    /// Dispatch a method on an interface-typed base through the fixed
    /// interface vtable slot (the interface part of every vtable).
    pub(crate) fn gen_iface_method_call(
        &mut self,
        base: &Expr,
        iface_idx: usize,
        method: &str,
        args: &[Expr],
        frame: &mut Frame,
        span: Span,
    ) -> CompileResult<Ty> {
        let iface = &self.prog.interfaces[iface_idx];
        let mi = iface
            .methods
            .iter()
            .position(|m| m.name == method)
            .ok_or_else(|| {
                CompileError::new(
                    span,
                    format!("interface '{}' has no method '{}'", iface.name, method),
                )
            })?;
        let meth = &iface.methods[mi];
        if args.len() != meth.params.len() {
            return Err(CompileError::new(
                span,
                format!(
                    "interface method '{}' expects {} args, got {}",
                    method,
                    meth.params.len(),
                    args.len()
                ),
            ));
        }
        // The callee owns the base and argument references it receives.
        self.gen_expr_ro(base, frame)?;
        self.maybe_retain(base, frame);
        let saved = self.str_copy;
        for (i, a) in args.iter().enumerate() {
            self.str_copy = meth.params[i].1.as_ref() == Some(&Ty::Str);
            self.gen_expr(a, frame)?;
            self.maybe_retain(a, frame);
        }
        self.str_copy = saved;
        let total = args.len() + 1;
        if total > 6 {
            return Err(CompileError::new(
                span,
                "too many arguments for method (including this)",
            ));
        }
        self.pop_args(total);
        let slot = layout::interface_slot(self.prog, method).ok_or_else(|| {
            CompileError::new(span, format!("method '{}' not in vtable", method))
        })?;
        self.emit("\tmovq 8(%rdi), %r11");
        self.emit(&format!("\tmovq {}*8(%r11), %r12", slot));
        self.emit("\tcall *%r12");
        if meth.ret == Some(Ty::Void) {
            self.emit("\tmovq $0, %rax");
            self.emit("\tpush %rax");
            return Ok(Ty::Void);
        } else {
            self.emit("\tpush %rax");
            return Ok(meth.ret.clone().unwrap_or(Ty::Int));
        }
    }

    /// Pop n arguments (previously pushed in order) into the arg registers.
    pub(crate) fn pop_args(&mut self, n: usize) {
        for i in (0..n).rev() {
            self.emit(&format!("\tpop {}", ARGREGS[i]));
        }
    }
}
