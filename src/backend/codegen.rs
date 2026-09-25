mod accessor;
mod binop;
mod builtin;
mod call;
mod ctx;
mod expr;
mod func;
mod lvalue;
mod new;
mod stmt;

use crate::ast::{Accessor, BinOp, Expr, Program, Ty};
use crate::error::{CompileError, CompileResult};
use crate::backend::layout;
use std::collections::HashMap;

pub(crate) use ctx::{Ctx, Frame, Local};

pub(crate) const ARGREGS: [&str; 6] = ["%rdi", "%rsi", "%rdx", "%rcx", "%r8", "%r9"];

fn mangle(class_name: &str, method_name: &str) -> String {
    format!("{}_{}", class_name, method_name)
}

/// Struct index when a type is class-typed.
pub(crate) fn struct_idx_of(ty: &Ty) -> Option<usize> {
    if let Ty::Struct(i) = ty {
        Some(*i)
    } else {
        None
    }
}

/// True when the expression produces a *fresh* class object that owns its
/// own reference: `new` (StructLit), a user function call returning a class,
/// or a method call returning a class (getters return a retained copy).
/// Existing references (local read, `this`, field read) are not temporary.
pub(crate) fn is_temp_class(e: &Expr) -> bool {
    matches!(e, Expr::StructLit { .. } | Expr::Call { .. } | Expr::MethodCall { .. })
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

fn field_to_pascal(s: &str) -> String {
    let trimmed = s.trim_start_matches('_');
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| capitalize(part))
        .collect()
}

pub(crate) fn getter_name(field: &str) -> String {
    format!("get{}", field_to_pascal(field))
}

pub(crate) fn setter_name(field: &str) -> String {
    format!("set{}", field_to_pascal(field))
}

fn ty_name(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int => "int",
        Ty::Bool => "bool",
        Ty::Ptr => "*ptr",
        Ty::Str => "*str",
        Ty::Array => "array",
        Ty::Void => "void",
        Ty::Struct(_) => "class",
        Ty::Interface(_) => "interface",
        Ty::Enum(_) => "int",
        Ty::Param(_) => "param",
        Ty::Inst(_, _) => "class",
    }
}

fn binop_result(op: BinOp) -> Ty {
    match op {
        BinOp::Eq
        | BinOp::Ne
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::Le
        | BinOp::Ge
        | BinOp::And
        | BinOp::Or => Ty::Bool,
        _ => Ty::Int,
    }
}

pub(crate) fn e_span(e: &Expr) -> crate::span::Span {
    crate::parser::expr_span(e)
}

pub fn generate(prog: &Program) -> CompileResult<String> {
    let mut ctx = Ctx {
        prog,
        out: Vec::new(),
        rodata: Vec::new(),
        strn: 0,
        func_idx: HashMap::new(),
        struct_idx: HashMap::new(),
        method_map: HashMap::new(),
        stack_region: None,
        str_copy: false,
    };
    for (i, f) in prog.funcs.iter().enumerate() {
        ctx
            .func_idx
            .insert(f.name.clone(), i)
            .map(|_| ());
    }
    for (i, s) in prog.structs.iter().enumerate() {
        ctx.struct_idx.insert(s.name.clone(), i);
        let mut m = HashMap::new();
        for (mi, meth) in s.methods.iter().enumerate() {
            m.insert(meth.name.clone(), (i, mi));
        }
        ctx.method_map.insert(s.name.clone(), m);
    }
    ctx.emit(".section .text");
    for f in &prog.funcs {
        ctx.gen_func(f)?;
    }
    // methods
    for s in &prog.structs {
        for meth in &s.methods {
            ctx.gen_method(s, meth)?;
        }
    }
    // synthetic accessors for get/set/getset
    for s in &prog.structs {
        for f in &s.fields {
            match f.accessor {
                Accessor::Get => ctx.gen_getter(s, f)?,
                Accessor::Set => ctx.gen_setter(s, f)?,
                Accessor::GetSet => {
                    ctx.gen_getter(s, f)?;
                    ctx.gen_setter(s, f)?;
                }
                Accessor::None => {}
            }
        }
    }
    // per-class refcount release functions
    for (i, _s) in prog.structs.iter().enumerate() {
        ctx.emit_release_fn(i);
    }
    // concrete classes must override inherited abstract methods and
    // implement the methods of every interface they declare
    for (i, s) in prog.structs.iter().enumerate() {
        if s.is_abstract {
            continue;
        }
        // inherited abstract methods (walk the parent chain)
        let mut cur = i;
        loop {
            match layout::parent_idx(prog, cur) {
                Some(p) => {
                    for m in &prog.structs[p].methods {
                        if m.is_abstract && layout::vtable_slot_fn(prog, i, &m.name).is_none() {
                            return Err(CompileError::new(
                                s.span,
                                format!(
                                    "class '{}' does not implement abstract method '{}' from parent '{}'",
                                    s.name, m.name, prog.structs[p].name
                                ),
                            ));
                        }
                    }
                    cur = p;
                }
                None => break,
            }
        }
        // declared interfaces' methods
        for iname in &s.implements {
            if let Some(iface) = prog.interfaces.iter().find(|x| &x.name == iname) {
                for m in &iface.methods {
                    if layout::vtable_slot_fn(prog, i, &m.name).is_none() {
                        return Err(CompileError::new(
                            s.span,
                            format!(
                                "class '{}' does not implement interface method '{}' from '{}'",
                                s.name, m.name, iface.name
                            ),
                        ));
                    }
                }
            } else {
                return Err(CompileError::new(
                    s.span,
                    format!("class '{}' implements unknown interface '{}'", s.name, iname),
                ));
            }
        }
    }
    // the flint_unimplemented stub (target of un-overridden abstract slots)
    if prog
        .structs
        .iter()
        .enumerate()
        .any(|(i, _)| layout::has_vtable_slot(prog, i))
    {
        ctx.emit_flint_unimplemented();
    }
    // entry point
    ctx.emit(".globl _start");
    ctx.emit("_start:");
    ctx.emit("\tcall flint_ignore_sigpipe");
    ctx.emit("\tcall main");
    ctx.emit("\tmov %eax, %edi");
    ctx.emit("\tcall flint_exit");
    // bss (static fields)
    let has_static = prog
        .structs
        .iter()
        .any(|s| s.fields.iter().any(|f| f.is_static));
    if has_static {
        ctx.emit(".section .bss");
        for s in &prog.structs {
            for f in &s.fields {
                if f.is_static {
                    let sym = format!("static_{}_{}", s.name, f.name);
                    ctx.emit(&format!(".globl {}", sym));
                    ctx.emit(&format!(".type {}, @object", sym));
                    ctx.emit(&format!("{}:", sym));
                    ctx.emit(&format!(".zero 8"));
                    ctx.emit(&format!(".size {}, .-{}", sym, sym));
                }
            }
        }
    }
    // rodata (string data + vtables)
    let rodata = std::mem::take(&mut ctx.rodata);
    let has_vtable = prog
        .structs
        .iter()
        .enumerate()
        .any(|(i, _)| layout::has_vtable_slot(prog, i));
    if !rodata.is_empty() || has_vtable {
        ctx.emit(".section .rodata");
        for r in &rodata {
            ctx.emit(r);
        }
        for (i, _s) in prog.structs.iter().enumerate() {
            if layout::has_vtable_slot(prog, i) {
                ctx.emit_vtable(i);
            }
        }
    }
    Ok(ctx.out.join("\n"))
}

impl Ctx<'_> {
    /// Emit the vtable for one class (in .rodata).
    fn emit_vtable(&mut self, sidx: usize) {
        let sym = layout::vtable_symbol(self.prog, sidx);
        self.emit(&format!(".globl {}", sym));
        self.emit(&format!(".type {}, @object", sym));
        self.emit(&format!("{}:", sym));
        for name in layout::vtable_slots(self.prog, sidx) {
            match layout::vtable_slot_fn(self.prog, sidx, &name) {
                Some(fn_name) => self.emit(&format!(".quad {}", fn_name)),
                None => self.emit("\t.quad flint_unimplemented"),
            }
        }
        self.emit(&format!(".size {}, .-{}", sym, sym));
    }

    /// Emit the flint_unimplemented stub (calls flint_exit with code 1).
    fn emit_flint_unimplemented(&mut self) {
        self.emit(".globl flint_unimplemented");
        self.emit(".type flint_unimplemented, @function");
        self.emit("flint_unimplemented:");
        self.emit("\tmovq $1, %rdi");
        self.emit("\tcall flint_exit");
        self.emit("\tret");
        self.emit(".size flint_unimplemented, .-flint_unimplemented");
    }
}
