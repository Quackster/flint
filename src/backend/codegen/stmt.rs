use crate::ast::{Block, Expr, Stmt, Ty};
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use crate::backend::escape::LocalKind;
use super::{Ctx, Frame, Local, struct_idx_of};
use super::call::check_ptr_conforms;

impl Ctx<'_> {
    pub(crate) fn gen_block(&mut self, block: &Block, frame: &mut Frame) -> CompileResult<()> {
        for stmt in &block.stmts {
            self.gen_stmt(stmt, frame)?;
        }
        Ok(())
    }

    /// After a class value is pushed on the expression stack, retain it when
    /// the value is copied from an existing reference (local read, field
    /// read, this). Fresh `new` values and call results already own a
    /// reference and are not retained here. A cast copies the reference, so
    /// it retains when its inner value is an existing reference.
    pub(crate) fn maybe_retain(&mut self, e: &Expr, frame: &Frame) {
        if self.should_retain(e, frame) {
            self.emit("\tmov (%rsp), %rdi");
            self.emit("\tcall flint_retain");
        }
    }

    pub(crate) fn should_retain(&self, e: &Expr, frame: &Frame) -> bool {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    l.kind == LocalKind::Heap || matches!(l.ty, Ty::Interface(_))
                } else if let Some(sidx) = frame.this_class {
                    // implicit this field (bare field name in a method)
                    self.struct_field_type(sidx, name)
                        .map_or(false, |t| matches!(t, Ty::Struct(_) | Ty::Interface(_)))
                } else {
                    false
                }
            }
            Expr::This { .. } => frame.this_class.is_some(),
            Expr::SuperBase { .. } => frame.this_class.is_some(),
            Expr::Field { base, name, .. } => {
                self.expr_struct_type(base, frame)
                    .ok()
                    .flatten()
                    .and_then(|s| self.struct_field_type(s, name).ok())
                    .map_or(false, |t| matches!(t, Ty::Struct(_) | Ty::Interface(_)))
            }
            Expr::Cast { e, .. } => self.should_retain(e, frame),
            _ => false,
        }
    }

    /// True when an assignment target holds a string value.
    pub(crate) fn assign_target_is_str(&self, target: &Expr, frame: &Frame) -> CompileResult<bool> {
        match target {
            Expr::Ident { name, .. } => Ok(frame
                .find(name)
                .map_or(false, |l| l.ty == Ty::Str)),
            Expr::Field { base, name, .. } => {
                // Check for static field access: ClassName.staticField
                // (looked up directly — layout::all_fields skips statics)
                if let Some(sidx) = self.is_class_name(base) {
                    let mut s = sidx;
                    loop {
                        if let Some(f) = self
                            .prog
                            .structs[s]
                            .fields
                            .iter()
                            .find(|f| &f.name == name)
                        {
                            if f.is_static {
                                return Ok(f.ty == Ty::Str);
                            }
                        }
                        match crate::backend::layout::parent_idx(self.prog, s) {
                            Some(p) => s = p,
                            None => return Ok(false),
                        }
                    }
                }
                let s = self.base_struct_idx(base, frame)?;
                Ok(matches!(self.struct_field_type(s, name)?, Ty::Str))
            }
            _ => Ok(false),
        }
    }

    /// True when an assignment target is a static field (`ClassName.f`):
    /// no instance lookup applies to it.
    pub(crate) fn is_static_target(&self, target: &Expr) -> bool {
        if let Expr::Field { base, name, .. } = target {
            if let Some(sidx) = self.is_class_name(base) {
                return self.is_static_field(sidx, name);
            }
        }
        false
    }

    /// Struct index when an assignment target holds a class value.
    pub(crate) fn assign_target_class(&self, target: &Expr, frame: &Frame) -> CompileResult<Option<usize>> {
        match target {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    if l.kind == LocalKind::Heap {
                        return Ok(struct_idx_of(&l.ty));
                    }
                }
                Ok(None)
            }
            Expr::This { .. } => Ok(frame.this_class),
            Expr::Field { base, name, .. } => {
                // Check for static field access: ClassName.staticField
                if let Some(sidx) = self.is_class_name(base) {
                    if self.is_static_field(sidx, name) {
                        return Ok(None); // static fields are not class-typed
                    }
                }
                let s = self.base_struct_idx(base, frame)?;
                let ft = self.struct_field_type(s, name)?;
                Ok(struct_idx_of(&ft))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn gen_stmt(&mut self, stmt: &Stmt, frame: &mut Frame) -> CompileResult<()> {
        match stmt {
            Stmt::Decl { span, name, value, .. } => {
                // slot was pre-allocated by prewalk_decls
                let (loff, lkind, lregion, lsidx, lty) = {
                    let l = frame
                        .find(name)
                        .ok_or_else(|| CompileError::new(Span::new(0, 0), "internal: no slot"))?;
                    (l.off, l.kind, l.region, struct_idx_of(&l.ty), l.ty.clone())
                };
                if let (Some(roff), Some(s)) = (lregion, lsidx) {
                    let ns = self.prog.structs[s].fields.len() + 1;
                    self.stack_region = Some((roff, ns));
                }
                // a `string` slot stores a fresh writable copy of a literal
                let saved = self.str_copy;
                if matches!(lty, Ty::Str) {
                    self.str_copy = true;
                }
                let vty = self.gen_expr(value, frame)?;
                self.str_copy = saved;
                check_ptr_conforms(&vty, &Some(lty.clone()), *span, "cannot assign")?;
                self.maybe_retain(value, frame);
                let is_iface = matches!(lty, Ty::Interface(_));
                if lkind == LocalKind::Heap || is_iface {
                    // overwrite: release the old reference, then store
                    let rel: Option<String> = match (lsidx, is_iface) {
                        (Some(s), _) => Some(format!("flint_release_{}", self.prog.structs[s].name)),
                        (None, true) => Some("flint_release".to_string()),
                        (None, false) => None,
                    };
                    self.emit("\tpop %r10");
                    if let Some(rel) = rel {
                        self.emit(&format!("\tmovq {}(%rbp), %r11", loff));
                        // the generic release has no null check
                        let guard = (rel == "flint_release").then(|| {
                            let s = format!(".Lrel_skip{}", self.strn);
                            self.strn += 1;
                            s
                        });
                        if let Some(g) = &guard {
                            self.emit("\ttest %r11, %r11");
                            self.emit(&format!("\tjz {}", g));
                        }
                        self.emit("\tmov %r11, %rdi");
                        self.emit(&format!("\tcall {}", rel));
                        if let Some(g) = &guard {
                            self.emit(&format!("{}:", g));
                        }
                    }
                    self.emit(&format!("\tmov %r10, {}(%rbp)", loff));
                } else {
                    self.emit("\tpop %rax"); // result from stack
                    self.emit(&format!("\tmov %rax, {}(%rbp)", loff));
                }
                self.stack_region = None;
                Ok(())
            }
            Stmt::ExprStmt { expr, .. } => {
                let t = self.gen_expr(expr, frame)?;
                self.maybe_retain(expr, frame);
                if let Some(s) = struct_idx_of(&t) {
                    // a class value used as a statement: drop the reference
                    let cname = &self.prog.structs[s].name;
                    self.emit("\tpop %rax");
                    self.emit("\tmov %rax, %rdi");
                    self.emit(&format!("\tcall flint_release_{}", cname));
                } else {
                    self.emit("\tpop %rax"); // discard
                }
                Ok(())
            }
            Stmt::Assign { span, target, value, .. } => {
                // a `string` target stores a fresh writable copy of a literal
                let saved = self.str_copy;
                if self.assign_target_is_str(target, frame)? {
                    self.str_copy = true;
                }
                let vty = self.gen_expr(value, frame)?;
                self.str_copy = saved;
                // a typed-pointer target (e.g. `*Box p`) checks its value
                if let Ok(tty) = self.lvalue_value_type(target, frame) {
                    check_ptr_conforms(&vty, &Some(tty), *span, "cannot assign")?;
                }
                self.maybe_retain(value, frame);
                // class target: release the old reference before storing
                let target_class = self.assign_target_class(target, frame)?;
                let target_iface = !self.is_static_target(target)
                    && self.expr_interface_type(target, frame)?.is_some();
                self.emit("\tpop %r10"); // value -> r10 (caller-saved temp)
                self.emit_lvalue_addr(target, frame)?; // pushes address
                // Hold the address in %rdx: flint_release clobbers %rax (the
                // munmap syscall return), so the store must not depend on %rax.
                self.emit("\tpop %rdx"); // %rdx = address
                if let Some(s) = target_class {
                    let cname = &self.prog.structs[s].name;
                    self.emit("\tmovq (%rdx), %r11");
                    self.emit("\tmov %r11, %rdi");
                    self.emit(&format!("\tcall flint_release_{}", cname));
                } else if target_iface {
                    // interface target: release the old reference (the generic
                    // release has no null check and is a v1 no-op at refcount
                    // zero, so guard the call with a null test)
                    self.emit("\tmovq (%rdx), %r11");
                    self.emit("\ttest %r11, %r11");
                    let skip = format!(".Lrel_skip{}", self.strn);
                    self.strn += 1;
                    self.emit(&format!("\tjz {}", skip));
                    self.emit("\tmov %r11, %rdi");
                    self.emit("\tcall flint_release");
                    self.emit(&format!("{}:", skip));
                }
                self.emit("\tmov %r10, (%rdx)");
                let _ = vty;
                Ok(())
            }
            Stmt::If {
                cond,
                then,
                else_opt,
                span,
            } => {
                self.gen_expr(cond, frame)?;
                self.emit("\tpop %rax");
                self.emit("\ttest %rax, %rax");
                let else_label = format!(".Lif_else{}", self.strn);
                let end_label = format!(".Lif_end{}", self.strn);
                self.strn += 2;
                self.emit(&format!("\tjz {}", else_label));
                self.gen_block(then, frame)?;
                if let Some(eb) = else_opt {
                    self.emit(&format!("\tjmp {}", end_label));
                    self.emit(&format!("{}:", else_label));
                    self.gen_block(eb, frame)?;
                } else {
                    self.emit(&format!("{}:", else_label));
                }
                self.emit(&format!("{}:", end_label));
                let _ = span;
                Ok(())
            }
            Stmt::While { label, cond, body, .. } => {
                let start_label = format!(".Lwhile{}", self.strn);
                let end_label = format!(".Lwhile_e{}", self.strn);
                self.strn += 2;
                self.emit(&format!("{}:", start_label));
                self.gen_expr(cond, frame)?;
                self.emit("\tpop %rax");
                self.emit("\ttest %rax, %rax");
                self.emit(&format!("\tjz {}", end_label));
                frame
                    .jump_stack
                    .push((label.clone(), Some(start_label.clone()), end_label.clone()));
                self.gen_block(body, frame)?;
                frame.jump_stack.pop();
                self.emit(&format!("\tjmp {}", start_label));
                self.emit(&format!("{}:", end_label));
                Ok(())
            }
            Stmt::For {
                label,
                init,
                cond,
                update,
                body,
                ..
            } => {
                if let Some(i) = init {
                    self.gen_stmt(i, frame)?;
                }
                let start_label = format!(".Lfor{}", self.strn);
                let up_label = format!(".Lfor_u{}", self.strn);
                let end_label = format!(".Lfor_e{}", self.strn);
                self.strn += 3;
                self.emit(&format!("{}:", start_label));
                if let Some(c) = cond {
                    self.gen_expr(c, frame)?;
                    self.emit("\tpop %rax");
                    self.emit("\ttest %rax, %rax");
                    self.emit(&format!("\tjz {}", end_label));
                }
                // continue targets the update clause, not the loop start
                frame
                    .jump_stack
                    .push((label.clone(), Some(up_label.clone()), end_label.clone()));
                self.gen_block(body, frame)?;
                frame.jump_stack.pop();
                self.emit(&format!("{}:", up_label));
                if let Some(u) = update {
                    self.gen_stmt(u, frame)?;
                }
                self.emit(&format!("\tjmp {}", start_label));
                self.emit(&format!("{}:", end_label));
                Ok(())
            }
            Stmt::Break { span, label } => {
                let end = if let Some(l) = label {
                    let e = frame
                        .jump_stack
                        .iter()
                        .rev()
                        .find(|(lb, _, _)| lb.as_deref() == Some(l))
                        .map(|(_, _, e)| e.clone())
                        .ok_or_else(|| {
                            CompileError::new(*span, format!("unknown label '{}'", l))
                        })?;
                    e
                } else {
                    let (_, _, e) = frame.jump_stack.last().ok_or_else(|| {
                        CompileError::new(*span, "break outside a loop or switch")
                    })?;
                    e.clone()
                };
                self.emit(&format!("\tjmp {}", end));
                Ok(())
            }
            Stmt::Continue { span, label } => {
                let target = if let Some(l) = label {
                    let (_, c, _) = frame
                        .jump_stack
                        .iter()
                        .rev()
                        .find(|(lb, _, _)| lb.as_deref() == Some(l))
                        .ok_or_else(|| {
                            CompileError::new(*span, format!("unknown label '{}'", l))
                        })?;
                    c.clone().ok_or_else(|| {
                        CompileError::new(*span, format!("cannot 'continue' to '{}': not a loop", l))
                    })?
                } else {
                    frame
                        .jump_stack
                        .iter()
                        .rev()
                        .find_map(|(_, c, _)| c.clone())
                        .ok_or_else(|| CompileError::new(*span, "continue outside a loop"))?
                };
                self.emit(&format!("\tjmp {}", target));
                Ok(())
            }
            Stmt::Switch {
                target,
                cases,
                default,
                ..
            } => {
                self.gen_expr(target, frame)?;
                self.emit("\tpop %rax");
                let base = self.strn;
                let end_label = format!(".Lsw_end{}", base);
                let default_label = format!(".Lsw_def{}", base);
                let case_labels: Vec<String> = (0..cases.len())
                    .map(|i| format!(".Lsw_c{}_{}", base, i))
                    .collect();
                self.strn += cases.len() + 2;
                for (i, (v, _)) in cases.iter().enumerate() {
                    self.emit(&format!("\tcmp ${}, %rax", v));
                    self.emit(&format!("\tje {}", case_labels[i]));
                }
                if default.is_some() {
                    self.emit(&format!("\tjmp {}", default_label));
                }
                frame
                    .jump_stack
                    .push((None, None, end_label.clone()));
                // Case bodies are laid out sequentially with no jump between
                // them: a case with no break falls through to the next case,
                // like C. A break jumps to end_label via the jump stack.
                for (i, (_, block)) in cases.iter().enumerate() {
                    self.emit(&format!("{}:", case_labels[i]));
                    self.gen_block(block, frame)?;
                }
                if let Some(db) = default {
                    self.emit(&format!("{}:", default_label));
                    self.gen_block(db, frame)?;
                }
                frame.jump_stack.pop();
                self.emit(&format!("{}:", end_label));
                Ok(())
            }
            Stmt::CompoundAssign {
                span,
                target,
                op,
                value,
            } => {
                if !self.is_static_target(target)
                    && self.expr_interface_type(target, frame)?.is_some()
                {
                    // Event registration: `handler += impl` — store the
                    // reference (C#-style subscription), releasing the
                    // previously registered one if any.
                    self.gen_expr(value, frame)?;
                    self.maybe_retain(value, frame);
                    self.emit("\tpop %r10"); // value -> r10
                    self.emit_lvalue_addr(target, frame)?;
                    self.emit("\tpop %rdx"); // %rdx = address
                    self.emit("\tmovq (%rdx), %r11");
                    self.emit("\ttest %r11, %r11");
                    let skip = format!(".Lrel_skip{}", self.strn);
                    self.strn += 1;
                    self.emit(&format!("\tjz {}", skip));
                    self.emit("\tmov %r11, %rdi");
                    self.emit("\tcall flint_release");
                    self.emit(&format!("{}:", skip));
                    self.emit("\tmov %r10, (%rdx)");
                    return Ok(());
                }
                if self.assign_target_class(target, frame)?.is_some() {
                    return Err(CompileError::new(
                        *span,
                        "cannot compound-assign a class value",
                    ));
                }
                self.gen_expr(value, frame)?;
                self.emit("\tpop %rsi"); // rhs
                self.emit_lvalue_addr(target, frame)?;
                self.emit("\tpop %r10"); // address
                self.emit("\tmovq (%r10), %rdi"); // old value
                self.emit_binop(*op);
                self.emit("\tmov %rdi, (%r10)");
                Ok(())
            }
            Stmt::Throw { value, .. } => {
                // Evaluate the throw expression and store it in the global exception slot.
                // Jump to the end of the innermost try block.
                self.gen_expr(value, frame)?;
                self.maybe_retain(value, frame);
                self.emit_end_releases(frame);
                self.emit("\tpop %rax");
                self.emit("\tcall flint_throw");
                if let Some(target) = frame.try_end_stack.last() {
                    self.emit(&format!("\tjmp {}", target));
                } else {
                    // No enclosing try: return from the function.
                    self.emit("\tleave");
                    self.emit("\tret");
                }
                Ok(())
            }
            Stmt::TryCatch {
                try_block,
                catch_type,
                catch_var,
                catch_block,
                finally,
                ..
            } => {
                // Declare the catch variable as a local.
                let off = frame.slot();
                frame.locals.push(Local {
                    name: catch_var.clone(),
                    off,
                    ty: catch_type.clone(),
                    kind: LocalKind::Plain,
                    region: None,
                });
                let has_fin = finally.is_some();
                // A `finally` block must also run on `return`, so capture the
                // return value in a slot and flag whether a return fired.
                let (val_off, flag_off) = if has_fin {
                    (frame.slot(), frame.slot())
                } else {
                    (0, 0)
                };
                let check_label = format!(".Ltry_check{}", self.strn);
                let fin_label = format!(".Ltry_fin{}", self.strn);
                let end_label = format!(".Ltry_end{}", self.strn);
                self.strn += 3;
                if has_fin {
                    self.emit(&format!("\tmovq $0, {}(%rbp)", flag_off));
                    frame.ret_capture = Some((fin_label.clone(), val_off, flag_off));
                }
                // Clear the exception slot before the try block.
                self.emit("\tcall flint_exc_clear");
                // Push the try-check label onto the try stack so `throw` can jump here.
                frame.try_end_stack.push(check_label.clone());
                // Execute the try block.
                self.gen_block(try_block, frame)?;
                // Pop the try-check label from the try stack.
                frame.try_end_stack.pop();
                // Check if an exception was thrown.
                self.emit(&format!("{}:", check_label));
                self.emit("\tcall flint_exc_check");
                self.emit("\ttest %rax, %rax");
                if has_fin {
                    self.emit(&format!("\tjz {}", fin_label));
                } else {
                    self.emit(&format!("\tjz {}", end_label));
                }
                // Exception was thrown: load it into the catch variable.
                self.emit("\tcall flint_exc_get");
                self.emit(&format!("\tmov %rax, {}(%rbp)", off));
                // Execute the catch block.
                self.gen_block(catch_block, frame)?;
                // Clear the exception slot after the catch block.
                self.emit("\tcall flint_exc_clear");
                if has_fin {
                    // control never falls through here (both paths jump to the
                    // finally label), so the end label is emitted after it
                    self.emit(&format!("\tjmp {}", fin_label));
                } else {
                    self.emit(&format!("{}:", end_label));
                }
                if has_fin {
                    self.emit(&format!("{}:", fin_label));
                    frame.ret_capture = None;
                    // Execute the finally block.
                    self.gen_block(finally.as_ref().unwrap(), frame)?;
                    // A `return` fired in the try/catch blocks: exit with the
                    // captured value; otherwise continue after the try-catch.
                    self.emit(&format!("\tmovq {}(%rbp), %rax", flag_off));
                    self.emit("\ttest %rax, %rax");
                    self.emit(&format!("\tjz {}", end_label));
                    self.emit(&format!("\tmovq {}(%rbp), %rax", val_off));
                    self.emit("\tleave");
                    self.emit("\tret");
                    self.emit(&format!("{}:", end_label));
                }
                Ok(())
            }
            Stmt::Return { value, .. } => match value {
                Some(v) => {
                    // `return &x` for a local/parameter x: the frame dies with
                    // the function, so the address would dangle
                    if let Expr::AddrOf { span: aspan, e: inner } = v.as_ref() {
                        if let Expr::Ident { name, .. } = inner.as_ref() {
                            if frame.find(name).is_some() {
                                return Err(CompileError::new(
                                    *aspan,
                                    "cannot return the address of a local; it dies when the function returns",
                                ));
                            }
                        }
                    }
                    // a `string` return stores a fresh writable copy of a literal
                    let saved = self.str_copy;
                    if frame.ret_type == Some(Ty::Str) {
                        self.str_copy = true;
                    }
                    self.gen_expr(v, frame)?;
                    self.str_copy = saved;
                    self.maybe_retain(v, frame);
                    // a `finally` block must run before the return takes effect
                    if let Some((target, val_off, flag_off)) = &frame.ret_capture {
                        self.emit_end_releases(frame);
                        self.emit("\tpop %rax");
                        self.emit(&format!("\tmov %rax, {}(%rbp)", val_off));
                        self.emit(&format!("\tmovq $1, {}(%rbp)", flag_off));
                        self.emit(&format!("\tjmp {}", target));
                        return Ok(());
                    }
                    // the block-end releases are skipped on early return
                    self.emit_end_releases(frame);
                    self.emit("\tpop %rax");
                    self.emit("\tleave");
                    self.emit("\tret");
                    Ok(())
                }
                None => {
                    if let Some((target, val_off, flag_off)) = &frame.ret_capture {
                        self.emit_end_releases(frame);
                        self.emit("\txor %eax, %eax");
                        self.emit(&format!("\tmov %eax, {}(%rbp)", val_off));
                        self.emit(&format!("\tmovq $1, {}(%rbp)", flag_off));
                        self.emit(&format!("\tjmp {}", target));
                        return Ok(());
                    }
                    self.emit_end_releases(frame);
                    self.emit("\txor %eax, %eax");
                    self.emit("\tleave");
                    self.emit("\tret");
                    Ok(())
                }
            },
        }
    }
}
