// Escape analysis: decides per class-typed local whether its object is
// stack-allocated (non-escaping) or heap-allocated with reference counting.
//
// A local's object escapes (forces the heap) when any variable that refers to
// it is used in an escaping context:
//   - as an argument to a function or method call
//   - as the base (this) of a method call
//   - as a return value
//   - as the value stored into a class field
//   - address-taken (&x)
// or when the variable is the target of an assignment (its slot must hold a
// releasable heap address) or an alias outlives the creating decl.
use crate::ast::*;
use crate::span::Span;
use std::collections::{HashMap, HashSet};

use super::codegen::getter_name;
use super::layout;

/// Allocation kind of a class-typed local or parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    /// Not a class value (int, bool, raw pointer, str).
    Plain,
    /// Slot holds a heap object pointer: retain on copy, release on overwrite and death.
    Heap,
    /// This decl created a stack-allocated object; slot holds the region address.
    StackOwner,
    /// Slot holds the address of another local's stack object.
    StackRef,
}

/// Per-decl decision from escape analysis.
#[derive(Debug, Clone)]
pub struct DeclPlan {
    pub kind: LocalKind,
    /// Class index when the value is class-typed.
    pub sidx: Option<usize>,
    /// Region size in 8-byte slots (nfields+1) for StackOwner.
    pub nslots: Option<usize>,
    /// Type of the declared value.
    pub ty: Option<Ty>,
}

/// Whole-function plan from escape analysis.
pub struct FuncPlan {
    /// Decl decisions keyed by the decl statement span.
    pub decls: HashMap<Span, DeclPlan>,
    /// Total frame bytes (params + local slots + stack regions), 16-aligned.
    pub frame_bytes: usize,
    /// Kind per parameter slot (this first for methods).
    pub param_kinds: Vec<LocalKind>,
}

struct Var {
    span: Span,
    name: String,
    ty: Option<Ty>,
    kind: LocalKind,
    /// Id of the object the slot points at (if class-typed).
    obj: Option<usize>,
    /// True when this decl's value is a fresh `new` (creates the object).
    creator: bool,
    /// True when the decl's `new` dispatches to a ctor that escapes `this`.
    ctor_esc: bool,
    /// Block nesting depth at the decl.
    depth: usize,
    is_param: bool,
}

/// Shared frame layout: params first, then one 8-byte slot per decl, with an
/// optional 16-aligned stack-object region after a decl's slot. Returns total
/// frame bytes (16-aligned) and per-decl (slot offset, region offset).
pub fn frame_layout(nparams: usize, regions: &[usize]) -> (usize, Vec<(i64, Option<i64>)>) {
    let mut next = nparams * 8;
    let mut out = Vec::with_capacity(regions.len());
    for &r in regions {
        let slot = -((next + 8) as i64);
        next += 8;
        let mut region = None;
        if r > 0 {
            let pad = (16 - next % 16) % 16;
            next += pad;
            region = Some(-((next + r) as i64));
            next += r;
        }
        out.push((slot, region));
    }
    let total = if next == 0 {
        0
    } else {
        (next + 15) / 16 * 16
    };
    (total, out)
}

/// Visit every Decl statement in source order, including nested blocks.
pub fn for_each_decl<'a, F>(block: &'a Block, mut f: F)
where
    F: FnMut(&'a Stmt),
{
    fn walk_stmt<'a, F>(stmt: &'a Stmt, f: &mut F)
    where
        F: FnMut(&'a Stmt),
    {
        match stmt {
            Stmt::Decl { .. } => (*f)(stmt),
            Stmt::If {
                then,
                else_opt,
                ..
            } => {
                walk_block(then, f);
                if let Some(eb) = else_opt {
                    walk_block(eb, f);
                }
            }
            Stmt::While { body, .. } => walk_block(body, f),
            Stmt::For {
                init,
                body,
                update,
                ..
            } => {
                if let Some(i) = init {
                    walk_stmt(i, f);
                }
                walk_block(body, f);
                if let Some(u) = update {
                    walk_stmt(u, f);
                }
            }
            Stmt::Switch { cases, default, .. } => {
                for (_, b) in cases {
                    walk_block(b, f);
                }
                if let Some(db) = default {
                    walk_block(db, f);
                }
            }
            _ => {}
        }
    }
    fn walk_block<'a, F>(b: &'a Block, f: &mut F)
    where
        F: FnMut(&'a Stmt),
    {
        for s in &b.stmts {
            walk_stmt(s, f);
        }
    }
    walk_block(block, &mut f)
}

/// True when a method's body uses `this` in an escaping context: passed to a
/// call or method, stored into a field, or returned. Such a constructor would
/// leak its `this` past the object's scope, so `new` must heap-allocate.
pub fn method_escapes_this(prog: &Program, sidx: usize, meth: &MethodDef) -> bool {
    let mut struct_idx = HashMap::new();
    for (i, s) in prog.structs.iter().enumerate() {
        struct_idx.insert(s.name.clone(), i);
    }
    let mut func_idx = HashMap::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        func_idx.insert(f.name.clone(), i);
    }
    let mut a = Analyzer {
        prog,
        struct_idx,
        func_idx,
        this_class: Some(sidx),
        vars: Vec::new(),
        name_idx: HashMap::new(),
        marks: HashSet::new(),
        assign_targets: HashSet::new(),
        nobjs: 0,
        ctor_esc: HashMap::new(),
    };
    a.vars.push(Var {
        span: Span::new(0, 0),
        name: "this".to_string(),
        ty: Some(Ty::Struct(sidx)),
        kind: LocalKind::Heap,
        obj: None,
        creator: false,
        depth: 0,
        is_param: true,
        ctor_esc: false,
    });
    a.name_idx.insert("this".to_string(), 0);
    a.mark_block(&meth.body, 0);
    a.marks.contains("this") || a.assign_targets.contains("this")
}

struct Analyzer<'p> {
    prog: &'p Program,
    struct_idx: HashMap<String, usize>,
    func_idx: HashMap<String, usize>,
    this_class: Option<usize>,
    vars: Vec<Var>,
    name_idx: HashMap<String, usize>,
    marks: HashSet<String>,
    assign_targets: HashSet<String>,
    nobjs: usize,
    /// Memoized ctor this-escape facts, keyed by (class idx, ctor name).
    ctor_esc: HashMap<(usize, String), bool>,
}

impl<'p> Analyzer<'p> {
    fn mark(&mut self, name: &str) {
        self.marks.insert(name.to_string());
    }

    fn var_ty(&self, name: &str) -> Option<Ty> {
        self.name_idx.get(name).and_then(|&i| self.vars[i].ty.clone())
    }

    fn field_ty(&self, sidx: usize, name: &str) -> Option<Ty> {
        self.prog.structs[sidx]
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.ty.clone())
    }

    /// Lightweight type inference for a decl's value expression.
    fn expr_ty(&self, e: &Expr) -> Option<Ty> {
        match e {
            Expr::Ident { name, .. } => self.var_ty(name),
            Expr::This { .. } => self.this_class.map(Ty::Struct),
            Expr::Field { base, name, .. } => {
                let s = self.expr_ty(base)?;
                if let Ty::Struct(s) = s {
                    self.field_ty(s, name)
                } else {
                    None
                }
            }
            Expr::MethodCall { base, method, .. } => {
                let s = self.expr_ty(base)?;
                if let Ty::Struct(s) = s {
                    let cls = &self.prog.structs[s];
                    if let Some(m) = cls.methods.iter().find(|m| &m.name == method) {
                        return m.ret.clone();
                    }
                    for f in &cls.fields {
                        if f.accessor == Accessor::Get || f.accessor == Accessor::GetSet {
                            if getter_name(&f.name) == *method {
                                return Some(f.ty.clone());
                            }
                        }
                    }
                }
                None
            }
            Expr::Call { callee, .. } => {
                if callee.len() == 1 {
                    if let Some(i) = self.func_idx.get(&callee[0]) {
                        return self.prog.funcs[*i].ret.clone();
                    }
                }
                None
            }
            Expr::Await { e: inner, .. } => {
                // `await f(...)` yields the async `f`'s return type (0 for
                // void); a bare awaited value is an int.
                if let Expr::Call { callee, .. } = inner.as_ref() {
                    if callee.len() == 1 {
                        if let Some(i) = self.func_idx.get(&callee[0]) {
                            let f = &self.prog.funcs[*i];
                            if f.is_async {
                                return match &f.ret {
                                    Some(Ty::Void) | None => Some(Ty::Int),
                                    Some(r) => Some(r.clone()),
                                };
                            }
                        }
                    }
                }
                Some(Ty::Int)
            }
            Expr::StructLit { name, .. } => self.struct_idx.get(name).map(|&i| Ty::Struct(i)),
            Expr::Null { .. } => Some(Ty::Ptr(None)),
            _ => None,
        }
    }

    /// Record escaping uses found in an expression.
    fn mark_expr(&mut self, e: &Expr) {
        match e {
            Expr::This { .. } => {
                self.mark("this");
            }
            Expr::Call { args, .. } => {
                for a in args {
                    self.mark_expr(a);
                    if let Expr::Ident { name, .. } = a {
                        self.mark(name);
                    }
                }
            }
            Expr::MethodCall { base, args, .. } => {
                self.mark_expr(base);
                if let Expr::Ident { name, .. } = base.as_ref() {
                    self.mark(name);
                }
                for a in args {
                    self.mark_expr(a);
                    if let Expr::Ident { name, .. } = a {
                        self.mark(name);
                    }
                }
            }
            Expr::BinOp { l, r, .. } => {
                self.mark_expr(l);
                self.mark_expr(r);
            }
            Expr::UnOp { op, e: inner, .. } => {
                if *op == UnOp::Addr {
                    if let Expr::Ident { name, .. } = inner.as_ref() {
                        self.mark(name);
                    }
                }
                self.mark_expr(inner);
            }
            Expr::AddrOf { e: inner, .. } => {
                if let Expr::Ident { name, .. } = inner.as_ref() {
                    self.mark(name);
                }
            }
            Expr::IncrDecr { e: inner, .. } => self.mark_expr(inner),
            Expr::Deref { e: inner, .. } => self.mark_expr(inner),
            Expr::Index { base, idx, .. } => {
                self.mark_expr(base);
                self.mark_expr(idx);
            }
            Expr::Field { base, .. } => self.mark_expr(base),
            Expr::StructLit { fields, .. } => {
                for (_, v) in fields {
                    self.mark_expr(v);
                    if let Expr::Ident { name, .. } = v {
                        self.mark(name);
                    }
                }
            }
            Expr::ArrayLit { elems, .. } => {
                for v in elems {
                    self.mark_expr(v);
                }
            }
            Expr::Cond { cond, then, els, .. } => {
                self.mark_expr(cond);
                self.mark_expr(then);
                self.mark_expr(els);
            }
            Expr::Await { e: inner, .. } => {
                // The inner (async call arguments, or the awaited Task) is
                // used through the await; mark it like a plain use.
                self.mark_expr(inner);
            }
            Expr::Closure { captures, .. } => {
                for c in captures {
                    self.mark_expr(c);
                }
            }
            Expr::Lambda { body, .. } => self.mark_block(body, 0),
            _ => {}
        }
    }

    fn mark_stmt(&mut self, stmt: &Stmt, depth: usize) {
        match stmt {
            Stmt::Decl { value, .. } => self.mark_expr(value),
            Stmt::Assign { target, value, .. } => {
                if let Expr::Ident { name, .. } = target {
                    self.assign_targets.insert(name.clone());
                }
                if let Expr::Field { .. } = target {
                    if let Expr::Ident { name, .. } = value {
                        self.mark(name);
                    }
                }
                self.mark_expr(value);
            }
            Stmt::ExprStmt { expr, .. } => self.mark_expr(expr),
            Stmt::If {
                cond, then, else_opt, ..
            } => {
                self.mark_expr(cond);
                self.mark_block(then, depth + 1);
                if let Some(eb) = else_opt {
                    self.mark_block(eb, depth + 1);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.mark_expr(cond);
                self.mark_block(body, depth + 1);
            }
            Stmt::For {
                init,
                cond,
                update,
                body,
                ..
            } => {
                if let Some(i) = init {
                    self.mark_stmt(i, depth + 1);
                }
                if let Some(c) = cond {
                    self.mark_expr(c);
                }
                self.mark_block(body, depth + 1);
                if let Some(u) = update {
                    self.mark_stmt(u, depth + 1);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.mark_expr(v);
                    if let Expr::Ident { name, .. } = v.as_ref() {
                        self.mark(name);
                    }
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::CompoundAssign { target, value, .. } => {
                if let Expr::Ident { name, .. } = target {
                    self.assign_targets.insert(name.clone());
                }
                if let Expr::Field { .. } = target {
                    if let Expr::Ident { name, .. } = value {
                        self.mark(name);
                    }
                }
                self.mark_expr(value);
            }
            Stmt::Switch { target, cases, default, .. } => {
                self.mark_expr(target);
                for (_, b) in cases {
                    self.mark_block(b, depth + 1);
                }
                if let Some(db) = default {
                    self.mark_block(db, depth + 1);
                }
            }
            Stmt::Throw { value, .. } => {
                self.mark_expr(value);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                finally,
                ..
            } => {
                self.mark_block(try_block, depth + 1);
                self.mark_block(catch_block, depth + 1);
                if let Some(fb) = finally {
                    self.mark_block(fb, depth + 1);
                }
            }
        }
    }

    fn mark_block(&mut self, b: &Block, depth: usize) {
        for s in &b.stmts {
            self.mark_stmt(s, depth);
        }
    }

    /// True when a `new Name(...)` dispatches to a ctor that escapes `this`.
    fn structlit_ctor_esc(&mut self, name: &str, nargs: usize) -> bool {
        let sidx = match self.struct_idx.get(name) {
            Some(i) => *i,
            None => return false,
        };
        let ctor = match self.prog.structs[sidx]
            .methods
            .iter()
            .find(|m| m.is_ctor && m.params.len() == nargs)
        {
            Some(m) => m,
            None => return false,
        };
        let key = (sidx, ctor.name.clone());
        if let Some(&v) = self.ctor_esc.get(&key) {
            return v;
        }
        let v = method_escapes_this(self.prog, sidx, ctor);
        self.ctor_esc.insert(key, v);
        v
    }

    /// Collect decls in source order, resolving value types and object aliases.
    fn collect_decls(&mut self, stmt: &Stmt, depth: usize) {
        match stmt {
            Stmt::Decl {
                span, name, ty, value, ..
            } => {
                // declared type wins; value inference fills in the rest
                let ty = ty.clone().or_else(|| self.expr_ty(value));
                let (obj, creator, ctor_esc) = match value {
                    Expr::StructLit {
                        name: ref ln,
                        fields,
                        ..
                    } => {
                        self.nobjs += 1;
                        let ce = self.structlit_ctor_esc(ln, fields.len());
                        (Some(self.nobjs), true, ce)
                    }
                    Expr::Ident { name: ref n, .. } => {
                        if let Some(&idx) = self.name_idx.get(n) {
                            let v = &self.vars[idx];
                            if v.ty.as_ref().map_or(false, |t| matches!(t, Ty::Struct(_))) {
                                (v.obj, false, false)
                            } else {
                                (None, false, false)
                            }
                        } else {
                            (None, false, false)
                        }
                    }
                    _ => (None, false, false),
                };
                let idx = self.vars.len();
                self.vars.push(Var {
                    span: *span,
                    name: name.clone(),
                    ty,
                    kind: LocalKind::Plain,
                    obj,
                    creator,
                    ctor_esc,
                    depth,
                    is_param: false,
                });
                self.name_idx.insert(name.clone(), idx);
            }
            Stmt::If {
                then, else_opt, ..
            } => {
                self.collect_block(then, depth + 1);
                if let Some(eb) = else_opt {
                    self.collect_block(eb, depth + 1);
                }
            }
            Stmt::While { body, .. } => self.collect_block(body, depth + 1),
            Stmt::For {
                init, body, update, ..
            } => {
                if let Some(i) = init {
                    self.collect_decls(i, depth + 1);
                }
                self.collect_block(body, depth + 1);
                if let Some(u) = update {
                    self.collect_decls(u, depth + 1);
                }
            }
            Stmt::Switch { cases, default, .. } => {
                for (_, b) in cases {
                    self.collect_block(b, depth + 1);
                }
                if let Some(db) = default {
                    self.collect_block(db, depth + 1);
                }
            }
            _ => {}
        }
    }

    fn collect_block(&mut self, b: &Block, depth: usize) {
        for s in &b.stmts {
            self.collect_decls(s, depth);
        }
    }
}

/// Run escape analysis for one function or method body.
pub fn plan_func(
    prog: &Program,
    params: &[(String, Option<Ty>, Span)],
    body: &Block,
    is_method: bool,
    this_class: Option<usize>,
) -> FuncPlan {
    let mut struct_idx = HashMap::new();
    for (i, s) in prog.structs.iter().enumerate() {
        struct_idx.insert(s.name.clone(), i);
    }
    let mut func_idx = HashMap::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        func_idx.insert(f.name.clone(), i);
    }

    let mut a = Analyzer {
        prog,
        struct_idx,
        func_idx,
        this_class,
        vars: Vec::new(),
        name_idx: HashMap::new(),
        marks: HashSet::new(),
        assign_targets: HashSet::new(),
        nobjs: 0,
        ctor_esc: HashMap::new(),
    };

    // Parameters (and this, first for methods) are always heap references.
    if is_method {
        if let Some(c) = this_class {
            a.vars.push(Var {
                span: Span::new(0, 0),
                name: "this".to_string(),
                ty: Some(Ty::Struct(c)),
                kind: LocalKind::Heap,
                obj: None,
                creator: false,
                ctor_esc: false,
                depth: 0,
                is_param: true,
            });
            a.name_idx.insert("this".to_string(), 0);
        }
    }
    for (pname, pty, _) in params {
        let ty = pty.clone();
        let kind = match ty.as_ref() {
            Some(Ty::Struct(_)) => LocalKind::Heap,
            _ => LocalKind::Plain,
        };
        a.vars.push(Var {
            span: Span::new(0, 0),
            name: pname.clone(),
            ty,
            kind,
            obj: None,
            creator: false,
            ctor_esc: false,
            depth: 0,
            is_param: true,
        });
        a.name_idx.insert(pname.clone(), a.vars.len() - 1);
    }

    a.mark_block(body, 0);
    a.collect_block(body, 0);

    // An object escapes if any variable referring to it is marked or is an
    // assignment target (its slot must hold a releasable heap address), or
    // its ctor escapes `this` (the callee may keep the pointer).
    let mut obj_esc: HashSet<usize> = HashSet::new();
    for v in &a.vars {
        if let Some(o) = v.obj {
            if a.marks.contains(&v.name)
                || a.assign_targets.contains(&v.name)
                || v.ctor_esc
            {
                obj_esc.insert(o);
            }
        }
    }
    // An alias that outlives its creator forces the object to the heap.
    for v in &a.vars {
        if !v.creator && !v.is_param {
            if let Some(o) = v.obj {
                if let Some(c) = a.vars.iter().find(|c| c.creator && c.obj == Some(o)) {
                    if v.depth < c.depth {
                        obj_esc.insert(o);
                    }
                }
            }
        }
    }

    // Finalize kinds and build the plan.
    //
    // A non-escaping creator at the function top level (depth 0) is
    // stack-allocated: its region lives in the frame and dies with the
    // function, so no scope-end releases are needed. Aliases of it are
    // StackRef (plain addresses, no refcount traffic). Anything marked
    // escaping, ctor-escaping, or declared in a nested block is heap
    // refcounted. TODO(v2): extend to scoped stack objects with block-end
    // releases.
    let mut decls: HashMap<Span, DeclPlan> = HashMap::new();
    let mut regions: Vec<usize> = Vec::new();
    let mut param_kinds: Vec<LocalKind> = Vec::new();
    for v in &a.vars {
        if v.is_param {
            param_kinds.push(v.kind);
            continue;
        }
        let (kind, sidx, nslots) = match v.ty {
            Some(Ty::Struct(s)) => {
                let esc = v.obj.map_or(false, |o| obj_esc.contains(&o));
                if v.creator {
                    if esc || v.depth > 0 {
                        (LocalKind::Heap, Some(s), None)
                    } else {
                        (
                            LocalKind::StackOwner,
                            Some(s),
                            Some(layout::nslots(prog, s)),
                        )
                    }
                } else if v.obj.is_some() {
                    // alias of another local's object
                    (
                        if esc {
                            LocalKind::Heap
                        } else {
                            LocalKind::StackRef
                        },
                        Some(s),
                        None,
                    )
                } else {
                    (LocalKind::Heap, Some(s), None)
                }
            }
            _ => (LocalKind::Plain, None, None),
        };
        regions.push(nslots.map_or(0, |n| n * 8));
        decls.insert(
            v.span,
            DeclPlan {
                kind,
                sidx,
                nslots,
                ty: v.ty.clone(),
            },
        );
    }

    let nparams = params.len() + if is_method { 1 } else { 0 };
    let (frame_bytes, _) = frame_layout(nparams, &regions);
    FuncPlan {
        decls,
        frame_bytes,
        param_kinds,
    }
}
