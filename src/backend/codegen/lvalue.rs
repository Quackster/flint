use crate::ast::{Expr, FieldDef, MethodDef, Ty, Vis};
use crate::error::{CompileError, CompileResult};
use crate::span::Span;
use crate::backend::layout;

use super::{Ctx, Frame, e_span, is_temp_class};

impl Ctx<'_> {
    /// Emit code that pushes the *address* of an lvalue onto the stack.
    pub(crate) fn emit_lvalue_addr(&mut self, e: &Expr, frame: &mut Frame) -> CompileResult<()> {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    self.emit(&format!("\tlea {}(%rbp), %rax", l.off));
                    self.emit("\tpush %rax");
                    return Ok(());
                }
                // implicit this field?
                if let Some(sidx) = frame.this_class {
                    if let Ok(off) = self.struct_field_offset(sidx, name) {
                        self.check_field_access(sidx, name, frame, e_span(e))?;
                        let this_off = frame.this_offset.unwrap();
                        self.emit(&format!("\tmovq {}(%rbp), %rax", this_off));
                        self.emit(&format!("\tlea {}(%rax), %rax", off));
                        self.emit("\tpush %rax");
                        return Ok(());
                    }
                }
                Err(CompileError::new(e_span(e), format!("undefined variable '{}'", name)))
            }
            Expr::This { .. } => {
                // &this is address of this slot? But lvalue for this assignment not allowed; treat as load of this slot addr
                if let Some(off) = frame.this_offset {
                    self.emit(&format!("\tlea {}(%rbp), %rax", off));
                    self.emit("\tpush %rax");
                    Ok(())
                } else {
                    Err(CompileError::new(e_span(e), "'this' not available outside method"))
                }
            }
            Expr::Deref { e: inner, .. } => {
                // &(*p) == p
                let _ = self.gen_expr(inner, frame)?;
                Ok(())
            }
            Expr::Index { base, idx, .. } => {
                let bty = self.gen_expr(base, frame)?; // ptr
                self.gen_expr(idx, frame)?; // idx
                self.emit("\tpop %rdx"); // idx
                self.emit("\tpop %rax"); // base ptr
                // arrays carry a length header in slot 0; raw pointers do not
                let disp = if matches!(bty, Ty::Array) { 8 } else { 0 };
                self.emit(&format!("\tlea {}(%rax, %rdx, 8), %rax", disp));
                self.emit("\tpush %rax");
                Ok(())
            }
            Expr::Field { base, name, .. } => {
                // Check for static field access: ClassName.staticField
                if let Some(sidx) = self.is_class_name(base) {
                    if self.is_static_field(sidx, name) {
                        let sym = self.static_field_symbol(sidx, name);
                        self.emit(&format!("\tlea {}(%rip), %rax", sym));
                        self.emit("\tpush %rax");
                        return Ok(());
                    }
                }
                if is_temp_class(base) {
                    return Err(CompileError::new(
                        e_span(e),
                        "temporary value is not an assignment target",
                    ));
                }
                let sidx = self.base_struct_idx(base, frame)?;
                self.check_field_access(sidx, name, frame, e_span(e))?;
                let off = self.struct_field_offset(sidx, name)?;
                self.gen_expr(base, frame)?; // base pointer
                self.emit("\tpop %rax");
                self.emit(&format!("\tlea {}(%rax), %rax", off));
                self.emit("\tpush %rax");
                Ok(())
            }
            Expr::MethodCall { .. } => Err(CompileError::new(e_span(e), "method call is not an lvalue")),
            _ => Err(CompileError::new(
                e_span(e),
                "not a valid assignment target",
            )),
        }
    }

    /// The value type stored at an lvalue, used to type `&x` (e.g. `&s`
    /// for `string s` is a `*string`). Falls back to `int` where the type
    /// is not known (v1: trust the programmer; `emit_lvalue_addr` still
    /// diagnoses genuinely invalid lvalues).
    pub(crate) fn lvalue_value_type(&self, e: &Expr, frame: &Frame) -> CompileResult<Ty> {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    return Ok(l.ty.clone());
                }
                Ok(Ty::Int)
            }
            Expr::Field { base, name, .. } => match self.base_struct_idx(base, frame) {
                Ok(sidx) => self.struct_field_type(sidx, name),
                Err(_) => Ok(Ty::Int),
            },
            // `*p = ...`: the stored value is the pointee of p (v1: rank is
            // untracked, so `a[i] = ...` targets stay untyped `int`)
            Expr::Deref { e: inner, .. } => match self.static_expr_type(inner, frame) {
                Ok(Ty::Ptr(Some(p))) => Ok(*p),
                _ => Ok(Ty::Int),
            },
            _ => Ok(Ty::Int),
        }
    }

    /// Best-effort static type of an expression without codegen.
    fn static_expr_type(&self, e: &Expr, frame: &Frame) -> CompileResult<Ty> {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    return Ok(l.ty.clone());
                }
                Ok(Ty::Int)
            }
            Expr::Field { base, name, .. } => match self.base_struct_idx(base, frame) {
                Ok(sidx) => self.struct_field_type(sidx, name),
                Err(_) => Ok(Ty::Int),
            },
            _ => Ok(Ty::Int),
        }
    }

    /// Resolve a class name (as written in source) to its struct index. The
    /// monomorphizer prefixes the package to mangled class names
    /// (`std_Math`), so also try the `<pkg>_<name>` form.
    pub(crate) fn struct_idx_by_name(&self, name: &str) -> Option<usize> {
        if let Some(i) = self.struct_idx.get(name) {
            return Some(*i);
        }
        // A short name may refer to a package-qualified class whose mangled
        // name is `<pkg>_<name>`. Pick the shortest such key: a generic
        // instantiation (e.g. `std_List_std_Box`) also ends in `_Box`, but
        // the plain class (`std_Box`) is the shorter, intended target.
        let suffix = format!("_{}", name);
        self.struct_idx
            .iter()
            .filter(|(k, _)| k.as_str().ends_with(&suffix))
            .min_by_key(|(k, _)| (k.len(), k.as_str()))
            .map(|(_, v)| *v)
    }

    /// Check if an expression is a class name (for static field access).
    pub(crate) fn is_class_name(&self, e: &Expr) -> Option<usize> {
        if let Expr::Ident { name, .. } = e {
            self.struct_idx_by_name(name)
        } else {
            None
        }
    }

    /// Check if a field is static in the given class.
    pub(crate) fn is_static_field(&self, sidx: usize, field: &str) -> bool {
        // Check own fields first
        if let Some(f) = self.prog.structs[sidx].fields.iter().find(|f| f.name == field) {
            return f.is_static;
        }
        // Check parent chain
        if let Some(p) = layout::parent_idx(self.prog, sidx) {
            return self.is_static_field(p, field);
        }
        false
    }

    /// The global symbol name for a static field.
    pub(crate) fn static_field_symbol(&self, sidx: usize, field: &str) -> String {
        format!("static_{}_{}", self.prog.structs[sidx].name, field)
    }

    /// Resolve the struct index for a field-access base expression.
    pub(crate) fn base_struct_idx(&self, base: &Expr, frame: &Frame) -> CompileResult<usize> {
        match self.expr_struct_type(base, frame)? {
            Some(idx) => Ok(idx),
            None => Err(CompileError::new(
                e_span(base),
                "field access requires a class-typed base",
            )),
        }
    }

    /// Determine the struct index of an expression, if it denotes a struct.
    pub(crate) fn expr_struct_type(&self, e: &Expr, frame: &Frame) -> CompileResult<Option<usize>> {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    if let Ty::Struct(idx) = l.ty {
                        return Ok(Some(idx));
                    }
                }
                // implicit this field may be struct type
                if let Some(sidx) = frame.this_class {
                    if let Ok(ty) = self.struct_field_type(sidx, name) {
                        if let Ty::Struct(idx) = ty {
                            return Ok(Some(idx));
                        }
                    }
                }
                Ok(None)
            }
            Expr::This { .. } => {
                if let Some(c) = frame.this_class { Ok(Some(c)) } else { Ok(None) }
            }
            Expr::SuperBase { .. } => {
                match frame.this_class {
                    Some(c) => Ok(layout::parent_idx(self.prog, c)),
                    None => Ok(None),
                }
            }
            Expr::Field { base, name, .. } => {
                // Check for static field access: ClassName.staticField —
                // a class-typed static field denotes the class of its value
                // (a local with the class's spelling shadows it)
                let base_is_local = match base.as_ref() {
                    Expr::Ident { name, .. } => frame.find(name.as_str()).is_some(),
                    _ => false,
                };
                if !base_is_local {
                    if let Some(sidx) = self.is_class_name(base) {
                        if self.is_static_field(sidx, name) {
                            for (fname, fty) in layout::all_static_fields(self.prog, sidx) {
                                if fname == *name {
                                    if let Ty::Struct(idx) = fty {
                                        return Ok(Some(idx));
                                    }
                                    return Ok(None);
                                }
                            }
                        }
                    }
                }
                let sidx = self.expr_struct_type(base, frame)?.ok_or_else(|| {
                    CompileError::new(e_span(base), "field access requires a class-typed base")
                })?;
                let ft = self.struct_field_type(sidx, name)?;
                if let Ty::Struct(idx) = ft {
                    Ok(Some(idx))
                } else {
                    Ok(None)
                }
            }
            Expr::Cast { ty, .. } => {
                if let Ty::Struct(idx) = ty {
                    Ok(Some(*idx))
                } else {
                    Ok(None)
                }
            }
            Expr::StructLit { name, .. } => Ok(self.struct_idx.get(name).copied()),
            Expr::MethodCall { base, method, .. } => {
                // try to infer return type of method
                let sidx = self.expr_struct_type(base, frame)?.ok_or_else(|| {
                    CompileError::new(e_span(base), "method call requires a class-typed base")
                })?;
                // look up method (own or inherited)
                if let Some(m) = self.find_method(sidx, method) {
                    if let Some(Ty::Struct(idx)) = m.ret.clone() {
                        return Ok(Some(idx));
                    }
                    // if ret is void or int, not struct
                    return Ok(None);
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Determine the interface index of an expression, if it denotes an
    /// interface-typed value (a local or field of an interface type).
    pub(crate) fn expr_interface_type(&self, e: &Expr, frame: &Frame) -> CompileResult<Option<usize>> {
        let ty = match e {
            Expr::Ident { name, .. } => match frame.find(name) {
                Some(l) => Some(l.ty.clone()),
                None => match frame.this_class {
                    Some(sidx) => self.struct_field_type(sidx, name).ok(),
                    None => None,
                },
            },
            Expr::Field { base, name, .. } => match self.expr_struct_type(base, frame)? {
                Some(sidx) => self.struct_field_type(sidx, name).ok(),
                // static field access (ClassName.staticField) is not an
                // interface-typed value
                None if self.is_class_name(base).is_some() => None,
                None => {
                    return Err(CompileError::new(
                        e_span(base),
                        "field access requires a class-typed base",
                    ))
                }
            },
            _ => None,
        };
        if let Some(Ty::Interface(i)) = ty {
            Ok(Some(i))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn struct_field_offset(&self, sidx: usize, field: &str) -> CompileResult<i64> {
        let base = layout::field_base_offset(self.prog, sidx);
        for (i, (fname, _)) in layout::all_fields(self.prog, sidx).iter().enumerate() {
            if fname == field {
                return Ok(base + i as i64 * 8);
            }
        }
        Err(CompileError::new(
            Span::new(0, 0),
            format!("class has no field '{}'", field),
        ))
    }

    pub(crate) fn struct_field_type(&self, sidx: usize, field: &str) -> CompileResult<Ty> {
        for (fname, fty) in layout::all_fields(self.prog, sidx).iter() {
            if fname == field {
                return Ok(fty.clone());
            }
        }
        Err(CompileError::new(
            Span::new(0, 0),
            format!("class has no field '{}'", field),
        ))
    }

    /// Look up a field (own, then the parent chain).
    pub(crate) fn field_def<'b>(&'b self, sidx: usize, field: &str) -> Option<&'b FieldDef> {
        if let Some(f) = self.prog.structs[sidx].fields.iter().find(|f| f.name == field) {
            return Some(f);
        }
        if let Some(p) = layout::parent_idx(self.prog, sidx) {
            return self.field_def(p, field);
        }
        None
    }

    /// Look up a method (own, then the parent chain).
    pub(crate) fn find_method(&self, sidx: usize, name: &str) -> Option<&MethodDef> {
        if let Some(m) = self.prog.structs[sidx].methods.iter().find(|m| m.name == name) {
            return Some(m);
        }
        if let Some(p) = layout::parent_idx(self.prog, sidx) {
            return self.find_method(p, name);
        }
        None
    }

    pub(crate) fn check_field_access(&self, sidx: usize, field: &str, frame: &Frame, span: Span) -> CompileResult<()> {
        if let Some(f) = self.field_def(sidx, field) {
            if f.vis == Vis::Private {
                match frame.this_class {
                    Some(cur) if cur == sidx => Ok(()),
                    _ => Err(CompileError::new(
                        span,
                        format!("field '{}' of class '{}' is private", field, self.prog.structs[sidx].name),
                    )),
                }
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    pub(crate) fn check_method_access(&self, sidx: usize, method: &str, frame: &Frame, span: Span) -> CompileResult<()> {
        if let Some(m) = self.find_method(sidx, method) {
            if m.vis == Vis::Private {
                match frame.this_class {
                    Some(cur) if cur == sidx => Ok(()),
                    _ => Err(CompileError::new(
                        span,
                        format!("method '{}' of class '{}' is private", method, self.prog.structs[sidx].name),
                    )),
                }
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}
