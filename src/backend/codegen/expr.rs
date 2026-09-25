use crate::ast::{BinOp, Expr, Ty, UnOp};
use crate::error::{CompileError, CompileResult};
use crate::backend::escape::LocalKind;
use crate::backend::layout;

use super::{ARGREGS, Ctx, Frame, binop_result, e_span, is_temp_class, mangle, struct_idx_of, ty_name};

impl Ctx<'_> {
    /// Evaluate an expression, leaving its 64-bit result pushed on the stack.
    pub(crate) fn gen_expr(&mut self, e: &Expr, frame: &mut Frame) -> CompileResult<Ty> {
        match e {
            Expr::Int { value, .. } => {
                self.emit(&format!("\tmovq ${}, %rax", value));
                self.emit("\tpush %rax");
                Ok(Ty::Int)
            }
            Expr::EnumVariant { span, .. } => {
                // Should be resolved to Expr::Int by the mono pass.
                Err(CompileError::new(*span, "enum variant not resolved"))
            }
            Expr::Bool { value, .. } => {
                self.emit(&format!("\tmovq ${}, %rax", if *value { 1 } else { 0 }));
                self.emit("\tpush %rax");
                Ok(Ty::Bool)
            }
            Expr::Str { value, .. } => {
                let label = self.new_label(value);
                self.emit(&format!("\tlea {}(%rip), %rax", label));
                // stored into a writable string slot: fresh heap copy
                // (the read-only literal is left untouched)
                if self.str_copy {
                    self.emit("\tmov %rax, %rdi");
                    self.emit("\tcall flint_strcopy");
                }
                self.emit("\tpush %rax");
                Ok(Ty::Str)
            }
            Expr::Null { .. } => {
                self.emit("\txor %rax, %rax");
                self.emit("\tpush %rax");
                Ok(Ty::Ptr)
            }
            Expr::Cond {
                cond,
                then,
                els,
                ..
            } => {
                let base = self.strn;
                self.strn += 2;
                let else_label = format!(".Ltern_else{}", base);
                let end_label = format!(".Ltern_end{}", base);
                self.gen_expr(cond, frame)?;
                self.emit("\tpop %rax");
                self.emit("\ttest %rax, %rax");
                self.emit(&format!("\tjz {}", else_label));
                let t = self.gen_expr(then, frame)?;
                self.maybe_retain(then, frame);
                self.emit(&format!("\tjmp {}", end_label));
                self.emit(&format!("{}:", else_label));
                self.gen_expr(els, frame)?;
                self.maybe_retain(els, frame);
                self.emit(&format!("{}:", end_label));
                Ok(t)
            }
            Expr::This { span, .. } => {
                if let Some(off) = frame.this_offset {
                    self.emit(&format!("\tmovq {}(%rbp), %rax", off));
                    self.emit("\tpush %rax");
                    let c = frame.this_class.unwrap();
                    Ok(Ty::Struct(c))
                } else {
                    Err(CompileError::new(*span, "'this' is only valid inside a method"))
                }
            }
            Expr::Ident { name, span, .. } => {
                if let Some(l) = frame.find(name) {
                    self.emit(&format!("\tmovq {}(%rbp), %rax", l.off));
                    self.emit("\tpush %rax");
                    return Ok(l.ty.clone());
                }
                // implicit this field access?
                if let Some(sidx) = frame.this_class {
                    if let Ok(off) = self.struct_field_offset(sidx, name) {
                        self.check_field_access(sidx, name, frame, *span)?;
                        let this_off = frame.this_offset.unwrap();
                        self.emit(&format!("\tmovq {}(%rbp), %rax", this_off));
                        self.emit(&format!("\tmovq {}(%rax), %rax", off));
                        self.emit("\tpush %rax");
                        return Ok(self.struct_field_type(sidx, name)?);
                    }
                }
                Err(CompileError::new(*span, format!("undefined variable '{}'", name)))
            }
            Expr::BinOp { op, l, r, span } => {
                // `&&` / `||` short-circuit: the right side is only evaluated
                // when the left side does not decide the result.
                match op {
                    BinOp::And => {
                        let base = self.strn;
                        self.strn += 2;
                        let false_label = format!(".Land_f{}", base);
                        let end_label = format!(".Land_e{}", base);
                        self.gen_expr_ro(l, frame)?;
                        self.emit("\tpop %rax");
                        self.emit("\ttest %rax, %rax");
                        self.emit(&format!("\tjz {}", false_label));
                        self.gen_expr_ro(r, frame)?;
                        self.emit("\tpop %rax");
                        self.emit("\ttest %rax, %rax");
                        self.emit(&format!("\tjz {}", false_label));
                        self.emit("\tmovq $1, %rax");
                        self.emit(&format!("\tjmp {}", end_label));
                        self.emit(&format!("{}:", false_label));
                        self.emit("\txor %eax, %eax");
                        self.emit(&format!("{}:", end_label));
                        self.emit("\tpush %rax");
                        return Ok(Ty::Bool);
                    }
                    BinOp::Or => {
                        let base = self.strn;
                        self.strn += 2;
                        let true_label = format!(".Lor_t{}", base);
                        let end_label = format!(".Lor_e{}", base);
                        self.gen_expr_ro(l, frame)?;
                        self.emit("\tpop %rax");
                        self.emit("\ttest %rax, %rax");
                        self.emit(&format!("\tjnz {}", true_label));
                        self.gen_expr_ro(r, frame)?;
                        self.emit("\tpop %rax");
                        self.emit("\ttest %rax, %rax");
                        self.emit(&format!("\tjnz {}", true_label));
                        self.emit("\txor %eax, %eax");
                        self.emit(&format!("\tjmp {}", end_label));
                        self.emit(&format!("{}:", true_label));
                        self.emit("\tmovq $1, %rax");
                        self.emit(&format!("{}:", end_label));
                        self.emit("\tpush %rax");
                        return Ok(Ty::Bool);
                    }
                    _ => {}
                }
                // operands are consumed by the operator, never stored
                let lt = self.gen_expr_ro(l, frame)?;
                let rt = self.gen_expr_ro(r, frame)?;
                // `+` with two string-ish operands (string literal or a string
                // builtin result, which is Ty::Ptr) is concatenation. `Ptr + Ptr`,
                // `Str + Int`, and `Ptr + Int` stay pointer arithmetic.
                let str_concat = *op == BinOp::Add
                    && ((lt == Ty::Str && rt == Ty::Str)
                        || (lt == Ty::Str && rt == Ty::Ptr)
                        || (lt == Ty::Ptr && rt == Ty::Str));
                // `==` / `!=` on two strings compare contents, not pointers;
                // a `string` variable vs `null` (Ty::Ptr) stays a pointer test.
                let str_cmp = matches!(*op, BinOp::Eq | BinOp::Ne)
                    && lt == Ty::Str
                    && rt == Ty::Str;
                self.check_binop_types(*op, lt, rt, *span)?;
                self.emit("\tpop %rsi"); // r
                self.emit("\tpop %rdi"); // l
                if str_concat {
                    // `+` on two strings is concatenation, not pointer addition
                    self.emit("\tcall flint_strconcat");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Str);
                }
                if str_cmp {
                    self.emit("\tcall flint_strcmp");
                    self.emit("\tcmpq $0, %rax");
                    if *op == BinOp::Eq {
                        self.emit("\tsete %al");
                    } else {
                        self.emit("\tsetne %al");
                    }
                    self.emit("\tmovzbl %al, %eax");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Bool);
                }
                self.emit_binop(*op);
                self.emit("\tpush %rdi");
                Ok(binop_result(*op))
            }
            Expr::UnOp { op, e: inner, .. } => {
                let it = self.gen_expr_ro(inner, frame)?;
                match op {
                    UnOp::Neg => {
                        self.emit("\tpop %rdi");
                        self.emit("\tneg %rdi");
                        self.emit("\tmov %rdi, %rax");
                        self.emit("\tpush %rax");
                        Ok(Ty::Int)
                    }
                    UnOp::Not => {
                        self.emit("\tpop %rdi");
                        self.emit("\ttest %rdi, %rdi");
                        self.emit("\tsete %al");
                        self.emit("\tmovzbl %al, %eax");
                        self.emit("\tpush %rax");
                        let _ = it;
                        Ok(Ty::Bool)
                    }
                    UnOp::Deref => {
                        self.emit("\tpop %rdi");
                        self.emit("\tmovq (%rdi), %rax");
                        self.emit("\tpush %rax");
                        Ok(Ty::Int)
                    }
                    UnOp::FnAddr => {
                        // Should not reach here; FnAddr is handled in the Expr::FnAddr case
                        self.emit("\tmov %rsp, %rax");
                        Ok(Ty::Ptr)
                    }
                    UnOp::Addr => {
                        // value is at (%rsp); replace it with its own address
                        self.emit("\tmov %rsp, %rax");
                        self.emit("\tmov %rax, (%rsp)");
                        Ok(Ty::Ptr)
                    }
                }
            }
            Expr::IncrDecr { e: inner, inc, .. } => {
                // load the lvalue, add/sub 1, store back, push the new value
                self.emit_lvalue_addr(inner, frame)?;
                self.emit("\tpop %r10"); // address
                self.emit("\tmovq (%r10), %rax");
                if *inc {
                    self.emit("\taddq $1, %rax");
                } else {
                    self.emit("\tsubq $1, %rax");
                }
                self.emit("\tmovq %rax, (%r10)");
                self.emit("\tpush %rax");
                Ok(Ty::Int)
            }
            Expr::AddrOf { e: inner, span, .. } => {
                // class values are managed; raw pointers only
                if let Expr::Ident { name, .. } = inner.as_ref() {
                    if let Some(l) = frame.find(name) {
                        if l.kind == LocalKind::Heap {
                            return Err(CompileError::new(
                                *span,
                                "cannot take the address of a class value; it is managed",
                            ));
                        }
                    }
                }
                if let Expr::This { .. } = inner.as_ref() {
                    return Err(CompileError::new(
                        *span,
                        "cannot take the address of 'this'; it is managed",
                    ));
                }
                // address of the inner lvalue (not a temp copy)
                self.emit_lvalue_addr(inner, frame)?;
                Ok(Ty::Ptr)
            }
            Expr::FnAddr { e: inner, span } => {
                // Take the address of a function. The inner expression must be an identifier
                // that refers to a function.
                if let Expr::Ident { name, .. } = inner.as_ref() {
                    // Look up the function in the program and use its mangled name.
                    let mangled = self.lookup_func_name(name);
                    self.emit(&format!("\tlea {}(%rip), %rax", mangled));
                    self.emit("\tpush %rax");
                    Ok(Ty::Ptr)
                } else {
                    Err(CompileError::new(
                        *span,
                        "@ requires a function name",
                    ))
                }
            }
            Expr::Deref { e: inner, span } => {
                let t = self.gen_expr_ro(inner, frame)?;
                self.emit("\tpop %rdi");
                if matches!(t, Ty::Array) {
                    // arrays have a length header at slot 0; index them instead
                    return Err(CompileError::new(
                        *span,
                        "cannot dereference an array; index them",
                    ));
                }
                self.emit("\tmovq (%rdi), %rax");
                self.emit("\tpush %rax");
                if matches!(t, Ty::Ptr) {
                    Ok(Ty::Int)
                } else {
                    Ok(t)
                }
            }
            Expr::Call { callee, args, span, .. } => self.gen_call(callee, args, frame, *span),
            Expr::MethodCall { base, method, args, span } => self.gen_method_call(base, method, args, frame, *span),
            Expr::Index { base, idx, .. } => {
                let bty = self.gen_expr_ro(base, frame)?;
                self.gen_expr_ro(idx, frame)?;
                if !matches!(bty, Ty::Ptr | Ty::Str | Ty::Array) {
                    return Err(CompileError::new(
                        e_span(e),
                        format!("cannot index a {} value", ty_name(&bty)),
                    ));
                }
                self.emit("\tpop %rdx");
                self.emit("\tpop %rax");
                // arrays carry a length header in slot 0; raw pointers do not
                let disp = if matches!(bty, Ty::Array) { 8 } else { 0 };
                self.emit(&format!("\tmovq {}(%rax, %rdx, 8), %rax", disp));
                self.emit("\tpush %rax");
                // a row of a multi-dimensional array is itself an array
                if matches!(bty, Ty::Array) {
                    Ok(Ty::Array)
                } else {
                    Ok(Ty::Int)
                }
            }
            Expr::Field { base, name, .. } => {
                // Check for static field access: ClassName.staticField
                if let Some(sidx) = self.is_class_name(base) {
                    if self.is_static_field(sidx, name) {
                        let sym = self.static_field_symbol(sidx, name);
                        self.emit(&format!("\tmovq {}(%rip), %rax", sym));
                        self.emit("\tpush %rax");
                        // Look up the type from the static field
                        for (fname, fty) in layout::all_static_fields(self.prog, sidx) {
                            if fname == *name {
                                return Ok(fty);
                            }
                        }
                        return Err(CompileError::new(
                            e_span(e),
                            format!("class has no static field '{}'", name),
                        ));
                    }
                }
                let sidx = self.base_struct_idx(base, frame)?;
                self.check_field_access(sidx, name, frame, e_span(e))?;
                let off = self.struct_field_offset(sidx, name)?;
                self.gen_expr(base, frame)?; // base pointer
                self.emit("\tpop %rax");
                // Hold the field value in %rdx: flint_release clobbers %r11 (and
                // %rax), so the value must not live in a clobbered register.
                self.emit(&format!("\tmovq {}(%rax), %rdx", off)); // field value
                // a temporary base (call result, new) owns a reference: drop it
                if is_temp_class(base) {
                    self.emit("\tmov %rax, %rdi");
                    let cname = &self.prog.structs[sidx].name;
                    self.emit(&format!("\tcall flint_release_{}", cname));
                }
                self.emit("\tmov %rdx, %rax");
                self.emit("\tpush %rax");
                Ok(self.struct_field_type(sidx, name)?)
            }
            Expr::StructLit { name, fields, .. } => {
                let sidx = *self.struct_idx.get(name).ok_or_else(|| {
                    CompileError::new(e_span(e), format!("unknown class '{}'", name))
                })?;
                let s = &self.prog.structs[sidx];
                if s.is_abstract {
                    return Err(CompileError::new(
                        e_span(e),
                        format!("class '{}' is abstract and cannot be instantiated", name),
                    ));
                }
                // Consume the pending stack region (if the enclosing decl was
                // planned as a stack-allocated owner). Nested `new`s see None
                // and heap-allocate.
                let region = self.stack_region.take();
                let is_ctor_call = fields.iter().any(|(n, _)| n.starts_with("_ctor_arg"));
                // Check for ctor dispatch: if class has ctor with matching arity, prefer ctor
                let ctor_candidate = s
                    .methods
                    .iter()
                    .find(|m| m.is_ctor && m.params.len() == fields.len());
                if let Some(ctor) = ctor_candidate {
                    // ctor path: allocate object then call ctor (with header)
                    self.emit_new_base(sidx, region);
                    self.emit("\tpush %rax"); // base on stack
                    // args are generated in call order (parser zips them in
                    // order, synthetic or not); retain copies so the ctor's
                    // end-release balances. String-literal args for
                    // `string` params get a fresh writable copy.
                    let saved = self.str_copy;
                    for (i, (_, expr)) in fields.iter().enumerate() {
                        self.str_copy = ctor.params[i].1.as_ref() == Some(&Ty::Str);
                        self.gen_expr(expr, frame)?;
                        self.maybe_retain(expr, frame);
                    }
                    self.str_copy = saved;
                    // stack: base, arg0, arg1, ... (last arg on top)
                    let nargs = fields.len();
                    // pop args to rsi.. leaving base on stack
                    for i in (0..nargs).rev() {
                        self.emit(&format!("\tpop {}", ARGREGS[i + 1]));
                    }
                    // move base to rdi without popping
                    self.emit("\tmov (%rsp), %rdi");
                    let mangled = mangle(name, &ctor.name);
                    self.emit(&format!("\tcall {}", mangled));
                    // ctor returns void; new returns base
                    self.emit("\tpop %rax");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Struct(sidx));
                }
                // a class with a parent must define its own constructor (which
                // chains to the parent via super); there is no synthesized ctor
                if layout::parent_idx(self.prog, sidx).is_some() {
                    return Err(CompileError::new(
                        e_span(e),
                        format!(
                            "class '{}' has a parent but no constructor with {} args; add a 'new' method",
                            name,
                            fields.len()
                        ),
                    ));
                }
                if is_ctor_call {
                    let nargs = fields.len();
                    let has_any_ctor = s.methods.iter().any(|m| m.is_ctor);
                    if has_any_ctor {
                        return Err(CompileError::new(
                            e_span(e),
                            format!(
                                "class '{}' has no constructor with {} args (fields={})",
                                name, nargs, s.fields.len()
                            ),
                        ));
                    } else {
                        return Err(CompileError::new(
                            e_span(e),
                            format!(
                                "'{}' has {} fields, but {} argument(s) given",
                                name,
                                s.fields.len(),
                                nargs
                            ),
                        ));
                    }
                }
                // positional field-init path
                self.emit_new_base(sidx, region);
                self.emit("\tpush %rax"); // base on stack (survives field-value calls)
                let base = layout::field_base_offset(self.prog, sidx);
                let parent_total = layout::parent_idx(self.prog, sidx)
                    .map(|p| layout::total_fields(self.prog, p))
                    .unwrap_or(0);
                let saved = self.str_copy;
                for (i, f) in s.fields.iter().enumerate() {
                    // header + parent fields precede the own fields
                    let off = base + (parent_total + i) as i64 * 8;
                    match fields.iter().find(|(nm, _)| nm == &f.name) {
                        Some((_, expr)) => {
                            self.str_copy = f.ty == Ty::Str;
                            self.gen_expr(expr, frame)?; // value pushed above base
                            self.maybe_retain(expr, frame);
                            self.emit("\tpop %rax"); // value
                            self.emit("\tpop %r10"); // base
                            self.emit(&format!("\tmov %rax, {}(%r10)", off));
                            self.emit("\tpush %r10"); // base back
                        }
                        None => {
                            self.emit("\tpop %r10"); // base
                            self.emit(&format!("\tmovq $0, {}(%r10)", off));
                            self.emit("\tpush %r10"); // base back
                        }
                    }
                }
                self.str_copy = saved;
                // base is already on the stack from the last push (or the
                // initial push if no fields) — it is the result.
                Ok(Ty::Struct(sidx))
            }
            Expr::ArrayLit { elems, span, .. } => {
                // heap block of 8-byte slots; nested literals (multi-dim)
                // each allocate their own block. The base stays on the
                // expression stack so nested element codegen cannot clobber it.
                let n = elems.len();
                let size = ((n + 1) * 8) as i64; // slot 0 = length header
                self.emit(&format!("\tmovq ${}, %rdi", size));
                self.emit("\tcall flint_alloc");
                self.emit("\tpush %rax"); // base on stack (survives element codegen)
                self.emit("\tpop %r10"); // base
                self.emit(&format!("\tmovq ${}, (%r10)", n)); // length header
                self.emit("\tpush %r10"); // base back
                for (i, el) in elems.iter().enumerate() {
                    self.gen_expr_ro(el, frame)?; // value pushed above base
                    self.emit("\tpop %rax"); // value
                    self.emit("\tpop %r10"); // base
                    self.emit(&format!("\tmov %rax, {}(%r10)", (i + 1) * 8));
                    self.emit("\tpush %r10"); // base back
                }
                // base is already on the stack — it is the result
                let _ = span;
                Ok(Ty::Array)
            }
            Expr::SuperBase { span, .. } => {
                let c = frame
                    .this_class
                    .ok_or_else(|| CompileError::new(*span, "'super' is only valid inside a method"))?;
                let p = layout::parent_idx(self.prog, c).ok_or_else(|| {
                    CompileError::new(*span, "super requires a parent class")
                })?;
                let off = frame
                    .this_offset
                    .ok_or_else(|| CompileError::new(*span, "'super' is only valid inside a method"))?;
                self.emit(&format!("\tmovq {}(%rbp), %rax", off));
                self.emit("\tpush %rax");
                Ok(Ty::Struct(p))
            }
            Expr::SuperCall { span, args } => {
                let c = frame
                    .this_class
                    .ok_or_else(|| CompileError::new(*span, "'super' is only valid inside a method"))?;
                let p = layout::parent_idx(self.prog, c).ok_or_else(|| {
                    CompileError::new(*span, "super requires a parent class")
                })?;
                let pdef = &self.prog.structs[p];
                let ctor = pdef
                    .methods
                    .iter()
                    .find(|m| m.is_ctor && m.params.len() == args.len());
                if ctor.is_none() && !args.is_empty() {
                    return Err(CompileError::new(
                        *span,
                        format!(
                            "class '{}' has no constructor with {} args",
                            pdef.name,
                            args.len()
                        ),
                    ));
                }
                if let Some(ctor) = ctor {
                    let this_off = frame
                        .this_offset
                        .ok_or_else(|| CompileError::new(*span, "'super' is only valid inside a method"))?;
                    self.emit(&format!("\tmovq {}(%rbp), %rax", this_off));
                    self.emit("\tpush %rax");
                    let saved = self.str_copy;
                    for (i, a) in args.iter().enumerate() {
                        self.str_copy = ctor.params[i].1.as_ref() == Some(&Ty::Str);
                        self.gen_expr(a, frame)?;
                        self.maybe_retain(a, frame);
                    }
                    self.str_copy = saved;
                    let total = args.len() + 1;
                    if total > 6 {
                        return Err(CompileError::new(
                            *span,
                            "too many arguments for super call (including this)",
                        ));
                    }
                    self.pop_args(total);
                    let mangled = mangle(&pdef.name, &ctor.name);
                    self.emit(&format!("\tcall {}", mangled));
                    self.emit("\tmovq $0, %rax");
                    self.emit("\tpush %rax");
                }
                Ok(Ty::Void)
            }
            Expr::Cast { span, ty, e } => {
                let vty = self.gen_expr_ro(e, frame)?;
                // int/bool -> string: a fresh single-character string for the
                // byte value (`"hi" + (string)72` -> "hiH")
                if *ty == Ty::Str && matches!(vty, Ty::Int | Ty::Bool) {
                    self.emit("\tpop %r12"); // value
                    self.emit("\tpush %r12"); // keep it: flint_alloc clobbers r10/r11/rcx
                    self.emit("\tmovq $2, %rdi");
                    self.emit("\tcall flint_alloc");
                    self.emit("\tand $255, %r12");
                    self.emit("\tmov %r12, (%rax)"); // char; bytes 1+ zero (NUL)
                    self.emit("\tpop %r12");
                    self.emit("\tpush %rax");
                    return Ok(Ty::Str);
                }
                if let (Some(src_sidx), Some(tgt_sidx)) = (struct_idx_of(&vty), struct_idx_of(ty)) {
                    let src_name = &self.prog.structs[src_sidx].name;
                    let tgt_name = &self.prog.structs[tgt_sidx].name;
                    let valid = src_sidx == tgt_sidx
                        || layout::is_subtype(self.prog, src_sidx, tgt_name)
                        || layout::is_subtype(self.prog, tgt_sidx, src_name);
                    if !valid {
                        return Err(CompileError::new(
                            *span,
                            format!("cannot cast '{}' to '{}'", src_name, tgt_name),
                        ));
                    }
                }
                Ok(ty.clone())
            }
            Expr::Instanceof { span, e, ty } => {
                let (target_name, tgt_sidx) = match ty {
                    Ty::Struct(idx) => (self.prog.structs[*idx].name.clone(), Some(*idx)),
                    Ty::Interface(idx) => (self.prog.interfaces[*idx].name.clone(), None),
                    _ => {
                        return Err(CompileError::new(
                            *span,
                            "instanceof requires a class or interface type",
                        ))
                    }
                };
                if let Some(tsi) = tgt_sidx {
                    if !layout::has_vtable_slot(self.prog, tsi) {
                        return Err(CompileError::new(
                            *span,
                            format!(
                                "class '{}' has no subtypes; instanceof is not applicable",
                                target_name
                            ),
                        ));
                    }
                }
                self.gen_expr_ro(e, frame)?;
                self.emit("\tpop %rax"); // object
                let base = self.strn;
                self.strn += 1;
                let true_label = format!(".Linstof_t{}", base);
                let false_label = format!(".Linstof_f{}", base);
                let end_label = format!(".Linstof_e{}", base);
                self.emit("\ttest %rax, %rax");
                self.emit(&format!("\tjz {}", false_label));
                self.emit("\tmovq 8(%rax), %r11"); // vtable ptr
                for si in layout::subclasses_of(self.prog, &target_name) {
                    let vt = layout::vtable_symbol(self.prog, si);
                    self.emit(&format!("\tlea {}(%rip), %r12", vt));
                    self.emit("\tcmpq %r12, %r11");
                    self.emit(&format!("\tje {}", true_label));
                }
                self.emit(&format!("\tjmp {}", false_label));
                self.emit(&format!("{}:", true_label));
                self.emit("\tmovq $1, %rax");
                self.emit(&format!("\tjmp {}", end_label));
                self.emit(&format!("{}:", false_label));
                self.emit("\tmovq $0, %rax");
                self.emit(&format!("{}:", end_label));
                self.emit("\tpush %rax");
                Ok(Ty::Bool)
            }
        }
    }
}
