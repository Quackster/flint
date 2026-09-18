use crate::ast::{CollItem, CollKind, Expr, Ty};
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use super::{Ctx, Frame, coll_release_name};

/// Runtime operation of a collection method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollOp {
    ListAdd,
    ListGet,
    ListSet,
    ListRemove,
    ListContains,
    ListSort,
    ListReverse,
    ListInsert,
    ListJoin,
    QueuePush,
    QueuePop,
    QueuePeek,
    MapPut,
    MapGet,
    MapContains,
    MapRemove,
    MapKeys,
    MapValues,
    SetAdd,
    SetContains,
    SetRemove,
    Size,
    Clear,
}

impl Ctx<'_> {
    /// The built-in collection type an expression denotes, if any.
    pub(crate) fn coll_type_of(&self, e: &Expr, frame: &Frame) -> CompileResult<Option<Ty>> {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    if l.ty.coll_kind().is_some() {
                        return Ok(Some(l.ty.clone()));
                    }
                }
                // implicit this field access
                if let Some(sidx) = frame.this_class {
                    if let Ok(t) = self.struct_field_type(sidx, name) {
                        if t.coll_kind().is_some() {
                            return Ok(Some(t));
                        }
                    }
                }
                Ok(None)
            }
            Expr::Field { base, name, .. } => {
                let sidx = match self.expr_struct_type(base, frame)? {
                    Some(i) => i,
                    None => return Ok(None),
                };
                let ft = self.struct_field_type(sidx, name)?;
                Ok(ft.coll_kind().map(|_| ft))
            }
            _ => Ok(None),
        }
    }

    /// Storage flag for a value: 1 for int, 2 for string.
    /// (0 is reserved for "empty" in hash buckets and 3 for tombstone.)
    /// Unknown dynamic values default to int in v1.
    fn value_flag(&self, e: &Expr, frame: &Frame) -> u64 {
        const INT: u64 = 1;
        const STR: u64 = 2;
        match e {
            Expr::Str { .. } => STR,
            Expr::Ident { name, .. } => {
                if let Some(l) = frame.find(name) {
                    return if matches!(l.ty, Ty::Str | Ty::Ptr) { STR } else { INT };
                }
                if let Some(sidx) = frame.this_class {
                    if let Ok(t) = self.struct_field_type(sidx, name) {
                        return if matches!(t, Ty::Str | Ty::Ptr) { STR } else { INT };
                    }
                }
                INT
            }
            Expr::Call { callee, .. } => {
                if callee.len() == 1 {
                    if let Some(i) = self.func_idx.get(&callee[0]) {
                        if matches!(self.prog.funcs[*i].ret, Some(Ty::Str)) {
                            return STR;
                        }
                    }
                }
                INT
            }
            Expr::MethodCall { base, method, .. } => {
                if let Ok(Some(sidx)) = self.expr_struct_type(base, frame) {
                    if let Some(m) = self.find_method(sidx, method) {
                        if matches!(m.ret, Some(Ty::Str)) {
                            return STR;
                        }
                    }
                }
                INT
            }
            _ => INT,
        }
    }

    /// True when the collection's element/value type at `idx` is a class
    /// object (Ty::Struct / Ty::Interface). Such values are heap objects with a
    /// refcount at offset 0, so storing one into (or retrieving one from) the
    /// collection must flint_retain it: the collection is never freed, and the
    /// caller's local owns an independent reference released at scope end.
    /// Without this, two locals aliasing one object (e.g. `a = l.get(0)` where
    /// `l` holds `d`) double-free at scope end.
    fn coll_elem_obj(&self, cty: &Ty, idx: usize) -> bool {
        match cty {
            Ty::Coll(_, args) => matches!(
                args.get(idx),
                Some(Ty::Struct(_)) | Some(Ty::Interface(_))
            ),
            _ => false,
        }
    }

    /// Element (or value, for maps) type of a typed collection, or None when
    /// the collection is untyped (element type not statically known).
    pub(crate) fn coll_elem_type(&self, cty: &Ty, is_map: bool) -> Option<Ty> {
        match cty {
            Ty::Coll(_, args) => {
                let t = if is_map { args.get(1) } else { args.get(0) };
                t.cloned()
            }
            _ => None,
        }
    }

    /// True when the element/value type is a concrete class (has a per-class
    /// release function the collection can call to drop its references).
    fn is_obj_elem(ety: Option<&Ty>) -> bool {
        matches!(ety, Some(Ty::Struct(_)))
    }

    /// Load the collection's `elem_release` (a per-class release function) into
    /// %rsi; 0 for non-object element types (the runtime then skips releasing
    /// elements on destroy, so the collection is a borrowing container).
    fn emit_elem_release(&mut self, ety: Option<&Ty>) {
        if let Some(Ty::Struct(i)) = ety {
            let cname = &self.prog.structs[*i].name;
            self.emit(&format!("\tlea flint_release_{}(%rip), %rsi", cname));
        } else {
            self.emit("\txor %rsi, %rsi");
        }
    }

    /// (key flag, value flag) for a typed collection; None if untyped.
    /// Flags: 1 = int, 2 = string.
    pub(crate) fn coll_flags(&self, cty: &Ty) -> Option<(u64, u64)> {
        const INT: u64 = 1;
        const STR: u64 = 2;
        let flag = |t: &Ty| if matches!(t, Ty::Str | Ty::Ptr) { STR } else { INT };
        match cty {
            Ty::Coll(k, args) if !args.is_empty() => match k {
                CollKind::List | CollKind::Queue | CollKind::Set => {
                    let f = flag(args.first()?);
                    Some((f, f))
                }
                CollKind::Map => {
                    let kf = flag(args.first()?);
                    let vf = args.get(1).map(flag).unwrap_or(kf);
                    Some((kf, vf))
                }
            },
            _ => None,
        }
    }

    /// Dispatch a method call on a built-in collection.
    pub(crate) fn gen_coll_method(
        &mut self,
        base: &Expr,
        cty: Ty,
        method: &str,
        args: &[Expr],
        frame: &mut Frame,
        span: Span,
    ) -> CompileResult<Ty> {
        let kind = cty.coll_kind().unwrap();
        let (arity, op) = match (kind, method) {
            (CollKind::List, "add") => (1, CollOp::ListAdd),
            (CollKind::List, "get") => (1, CollOp::ListGet),
            (CollKind::List, "set") => (2, CollOp::ListSet),
            (CollKind::List, "remove") => (1, CollOp::ListRemove),
            (CollKind::List, "contains") => (1, CollOp::ListContains),
            (CollKind::List, "sort") => (0, CollOp::ListSort),
            (CollKind::List, "reverse") => (0, CollOp::ListReverse),
            (CollKind::List, "insert") => (2, CollOp::ListInsert),
            (CollKind::List, "join") => (1, CollOp::ListJoin),
            (CollKind::List, "size") => (0, CollOp::Size),
            (CollKind::List, "clear") => (0, CollOp::Clear),
            (CollKind::Queue, "push") => (1, CollOp::QueuePush),
            (CollKind::Queue, "pop") => (0, CollOp::QueuePop),
            (CollKind::Queue, "peek") => (0, CollOp::QueuePeek),
            (CollKind::Queue, "size") => (0, CollOp::Size),
            (CollKind::Queue, "clear") => (0, CollOp::Clear),
            (CollKind::Map, "put") => (2, CollOp::MapPut),
            (CollKind::Map, "get") => (1, CollOp::MapGet),
            (CollKind::Map, "contains") => (1, CollOp::MapContains),
            (CollKind::Map, "remove") => (1, CollOp::MapRemove),
            (CollKind::Map, "size") => (0, CollOp::Size),
            (CollKind::Map, "clear") => (0, CollOp::Clear),
            (CollKind::Map, "keys") => (0, CollOp::MapKeys),
            (CollKind::Map, "values") => (0, CollOp::MapValues),
            (CollKind::Set, "add") => (1, CollOp::SetAdd),
            (CollKind::Set, "contains") => (1, CollOp::SetContains),
            (CollKind::Set, "remove") => (1, CollOp::SetRemove),
            (CollKind::Set, "size") => (0, CollOp::Size),
            (CollKind::Set, "clear") => (0, CollOp::Clear),
            _ => {
                return Err(CompileError::new(
                    span,
                    format!("type '{}' has no method '{}'", kind.name(), method),
                ))
            }
        };
        if args.len() != arity {
            return Err(CompileError::new(
                span,
                format!(
                    "'{}' expects {} argument(s), got {}",
                    method,
                    arity,
                    args.len()
                ),
            ));
        }

        // base first (it stays below the args on the expression stack);
        // collection arguments share strings, so literals stay read-only
        self.gen_expr_ro(base, frame)?;
        for a in args {
            self.gen_expr_ro(a, frame)?;
        }

        // the value flavour (int or string) the caller receives; guided by the
        // collection's element type when typed, else the receiver's declared type
        const STR: u64 = 2;
        let flags = self.coll_flags(&cty);
        let want_str = match flags {
            Some((_, v)) => v == STR,
            None => matches!(self.coll_expect, Some(Ty::Str) | Some(Ty::Ptr)),
        };
        // typed element/key flags (0 = fall back to per-expression inference)
        let vflag = match flags {
            Some((_, v)) => v,
            None => 0,
        };
        let kflag = match flags {
            Some((k, _)) => k,
            None => 0,
        };

        match op {
            CollOp::ListAdd | CollOp::QueuePush => {
                self.emit("\tpop %rsi"); // value
                if self.coll_elem_obj(&cty, 0) {
                    self.emit("\tmov %rsi, %rdi");
                    self.emit("\tcall flint_retain"); // collection owns a reference
                }
                self.emit("\tmov (%rsp), %rdi"); // collection
                self.emit(&format!("\tcall {}", if op == CollOp::ListAdd { "flint_list_add" } else { "flint_queue_push" }));
                self.emit("\tpop %r10"); // collection back out
                self.emit("\tpush %rax"); // 1/0
                Ok(Ty::Int)
            }
            CollOp::ListGet => {
                self.emit("\tpop %rsi"); // index
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_get");
                if self.coll_elem_obj(&cty, 0) {
                    self.emit("\tmov %rax, %rdi");
                    self.emit("\tcall flint_retain"); // caller owns a reference
                }
                self.emit("\tpop %r10");
                self.emit("\tpush %rax");
                Ok(if want_str { Ty::Str } else { Ty::Int })
            }
            CollOp::ListSet => {
                self.emit("\tpop %rdx"); // value
                if self.coll_elem_obj(&cty, 0) {
                    self.emit("\tmov %rdx, %rdi");
                    self.emit("\tcall flint_retain"); // collection owns a reference
                }
                self.emit("\tpop %rsi"); // index
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_set");
                self.emit("\tpop %rax"); // collection
                self.emit("\txor %eax, %eax");
                self.emit("\tpush %rax"); // void result (stack balance)
                Ok(Ty::Int)
            }
            CollOp::ListRemove => {
                self.emit("\tpop %rsi"); // index
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_remove");
                self.emit("\tpop %rax"); // collection
                self.emit("\txor %eax, %eax");
                self.emit("\tpush %rax"); // void result (stack balance)
                Ok(Ty::Int)
            }
            CollOp::ListContains => {
                let ef = if vflag != 0 { vflag } else { self.value_flag(&args[0], frame) };
                self.emit(&format!("\tmovq ${}, %rdx", ef));
                self.emit("\tpop %rsi"); // value
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_contains");
                self.emit("\tpop %r10");
                self.emit("\tpush %rax"); // 1/0
                Ok(Ty::Int)
            }
            CollOp::ListSort => {
                // string elements (a `list<string>`) sort by content
                let sf = if vflag == STR { 1 } else { 0 };
                self.emit(&format!("\tmovq ${}, %rsi", sf));
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_sort");
                self.emit("\tpop %rax"); // list
                self.emit("\txor %eax, %eax");
                self.emit("\tpush %rax"); // void result (stack balance)
                Ok(Ty::Int)
            }
            CollOp::ListReverse => {
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_reverse");
                self.emit("\tpop %rax"); // list
                self.emit("\txor %eax, %eax");
                self.emit("\tpush %rax"); // void result (stack balance)
                Ok(Ty::Int)
            }
            CollOp::ListInsert => {
                self.emit("\tpop %rdx"); // value
                if self.coll_elem_obj(&cty, 0) {
                    self.emit("\tmov %rdx, %rdi");
                    self.emit("\tcall flint_retain"); // collection owns a reference
                }
                self.emit("\tpop %rsi"); // index
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_insert");
                self.emit("\tpop %rax"); // list
                self.emit("\txor %eax, %eax");
                self.emit("\tpush %rax"); // void result (stack balance)
                Ok(Ty::Int)
            }
            CollOp::ListJoin => {
                // string elements (a `list<string>`) join by content
                let jf = if vflag == STR { 1 } else { 0 };
                self.emit("\tpop %rsi"); // sep
                self.emit(&format!("\tmovq ${}, %rdx", jf));
                self.emit("\tmov (%rsp), %rdi"); // list
                self.emit("\tcall flint_list_join");
                self.emit("\tpop %r10"); // list
                self.emit("\tpush %rax"); // fresh string
                Ok(Ty::Str)
            }
            CollOp::QueuePop | CollOp::QueuePeek => {
                self.emit("\tmov (%rsp), %rdi"); // queue
                self.emit(&format!("\tcall {}", if op == CollOp::QueuePop { "flint_queue_pop" } else { "flint_queue_peek" }));
                if self.coll_elem_obj(&cty, 0) {
                    self.emit("\tmov %rax, %rdi");
                    self.emit("\tcall flint_retain"); // caller owns a reference
                }
                self.emit("\tpop %r10");
                self.emit("\tpush %rax");
                Ok(if want_str { Ty::Str } else { Ty::Int })
            }
            CollOp::MapPut => {
                let vf = if vflag != 0 { vflag } else { self.value_flag(&args[1], frame) };
                let kf = if kflag != 0 { kflag } else { self.value_flag(&args[0], frame) };
                self.emit(&format!("\tmovq ${}, %r8", vf));
                self.emit(&format!("\tmovq ${}, %rdx", kf));
                self.emit("\tpop %rcx"); // value
                if self.coll_elem_obj(&cty, 1) {
                    self.emit("\tmov %rcx, %rdi");
                    self.emit("\tcall flint_retain"); // map owns a reference
                }
                self.emit("\tpop %rsi"); // key
                self.emit("\tmov (%rsp), %rdi"); // map
                self.emit("\tcall flint_hashmap_put");
                self.emit("\tpop %r10"); // map back out
                self.emit("\tpush %rax"); // 1/0
                Ok(Ty::Int)
            }
            CollOp::MapGet => {
                // the runtime's `want_str` arg is a 0/1 boolean (int=0, str=1);
                // it is compared as `want_str + 1` against the stored value flag.
                let kf = if kflag != 0 { kflag } else { self.value_flag(&args[0], frame) };
                self.emit(&format!("\tmovq ${}, %r8", u64::from(want_str)));
                self.emit(&format!("\tmovq ${}, %rdx", kf));
                self.emit("\tpop %rsi"); // key
                self.emit("\tmov (%rsp), %rdi"); // map
                self.emit("\tcall flint_hashmap_get");
                if self.coll_elem_obj(&cty, 1) {
                    self.emit("\tmov %rax, %rdi");
                    self.emit("\tcall flint_retain"); // caller owns a reference
                }
                self.emit("\tpop %r10");
                self.emit("\tpush %rax");
                Ok(if want_str { Ty::Str } else { Ty::Int })
            }
            CollOp::MapContains | CollOp::SetContains => {
                let cf = if op == CollOp::MapContains {
                    if kflag != 0 { kflag } else { self.value_flag(&args[0], frame) }
                } else {
                    if vflag != 0 { vflag } else { self.value_flag(&args[0], frame) }
                };
                self.emit(&format!("\tmovq ${}, %rdx", cf));
                self.emit("\tpop %rsi"); // key / value
                self.emit("\tmov (%rsp), %rdi");
                self.emit(&format!(
                    "\tcall {}",
                    if op == CollOp::MapContains { "flint_hashmap_contains" } else { "flint_hashset_contains" }
                ));
                self.emit("\tpop %r10");
                self.emit("\tpush %rax"); // 1/0
                Ok(Ty::Int)
            }
            CollOp::MapRemove | CollOp::SetRemove => {
                let cf = if op == CollOp::MapRemove {
                    if kflag != 0 { kflag } else { self.value_flag(&args[0], frame) }
                } else {
                    if vflag != 0 { vflag } else { self.value_flag(&args[0], frame) }
                };
                self.emit(&format!("\tmovq ${}, %rdx", cf));
                self.emit("\tpop %rsi"); // key / value
                self.emit("\tmov (%rsp), %rdi");
                self.emit(&format!(
                    "\tcall {}",
                    if op == CollOp::MapRemove { "flint_hashmap_remove" } else { "flint_hashset_remove" }
                ));
                self.emit("\tpop %r10");
                self.emit("\tpush %rax"); // 1/0
                Ok(Ty::Int)
            }
            CollOp::SetAdd => {
                let ef = if vflag != 0 { vflag } else { self.value_flag(&args[0], frame) };
                self.emit(&format!("\tmovq ${}, %rdx", ef));
                self.emit("\tpop %rsi"); // value
                if self.coll_elem_obj(&cty, 0) {
                    self.emit("\tmov %rsi, %rdi");
                    self.emit("\tcall flint_retain"); // set owns a reference
                }
                self.emit("\tmov (%rsp), %rdi"); // set
                self.emit("\tcall flint_hashset_add");
                self.emit("\tpop %r10");
                self.emit("\tpush %rax"); // 1/0
                Ok(Ty::Int)
            }
            CollOp::MapKeys | CollOp::MapValues => {
                self.emit("\tmov (%rsp), %rdi"); // map
                self.emit(&format!(
                    "\tcall {}",
                    if op == CollOp::MapKeys { "flint_hashmap_keys" } else { "flint_hashmap_values" }
                ));
                self.emit("\tpop %r10");
                self.emit("\tpush %rax"); // fresh list
                Ok(Ty::List)
            }
            CollOp::Size => {
                self.emit("\tmovq (%rsp), %rdi");
                self.emit("\tmovq 8(%rdi), %rax"); // slot 1 is the size (slot 0 = refcount)
                self.emit("\tpop %r10");
                self.emit("\tpush %rax");
                Ok(Ty::Int)
            }
            CollOp::Clear => {
                self.emit("\tmov (%rsp), %rdi"); // collection
                self.emit(&format!(
                    "\tcall {}",
                    match kind {
                        CollKind::List => "flint_list_clear",
                        CollKind::Queue => "flint_queue_clear",
                        CollKind::Map => "flint_hashmap_clear",
                        CollKind::Set => "flint_hashset_clear",
                    }
                ));
                self.emit("\tpop %rax"); // collection
                self.emit("\txor %eax, %eax");
                self.emit("\tpush %rax"); // void result (stack balance)
                Ok(Ty::Int)
            }
        }
    }

    /// Build a collection from literal items, leaving it on the expression
    /// stack. `kind` is resolved by the caller (declared type or inference).
    pub(crate) fn gen_colllit(
        &mut self,
        kind: CollKind,
        items: &[CollItem],
        ety: Option<Ty>,
        frame: &mut Frame,
    ) -> CompileResult<Ty> {
        let n = items.len() as i64;
        let owns = Self::is_obj_elem(ety.as_ref());
        match kind {
            CollKind::List | CollKind::Queue | CollKind::Set => {
                for it in items {
                    if let CollItem::Pair(_, _) = it {
                        return Err(CompileError::new(
                            Span::new(0, 0),
                            format!(
                                "{} literals cannot contain 'key: value' pairs",
                                kind.name()
                            ),
                        ));
                    }
                }
                let (new_fn, add_fn, flagged, ty) = match kind {
                    CollKind::List => ("flint_list_new", "flint_list_add", false, Ty::List),
                    CollKind::Queue => ("flint_queue_new", "flint_queue_push", false, Ty::Queue),
                    _ => ("flint_hashset_new", "flint_hashset_add", true, Ty::HashSet),
                };
                self.emit(&format!("\tmovq ${}, %rdi", n));
                self.emit_elem_release(ety.as_ref());
                self.emit(&format!("\tcall {}", new_fn));
                self.emit("\tpush %rax"); // collection (survives element codegen)
                for it in items {
                    if let CollItem::Elem(e) = it {
                        self.gen_expr_ro(e, frame)?;
                        if owns {
                            self.maybe_retain(e, frame); // collection owns a reference
                        }
                        if flagged {
                            self.emit(&format!("\tmovq ${}, %rdx", self.value_flag(e, frame)));
                        }
                        self.emit("\tpop %rsi"); // value
                        self.emit("\tmov (%rsp), %rdi"); // collection
                        self.emit(&format!("\tcall {}", add_fn));
                    }
                }
                Ok(ty)
            }
            CollKind::Map => {
                for it in items {
                    if let CollItem::Elem(_) = it {
                        return Err(CompileError::new(
                            Span::new(0, 0),
                            "hashmap literal items must be 'key: value' pairs",
                        ));
                    }
                }
                self.emit(&format!("\tmovq ${}, %rdi", n));
                self.emit_elem_release(ety.as_ref());
                self.emit("\tcall flint_hashmap_new");
                self.emit("\tpush %rax"); // map
                for it in items {
                    if let CollItem::Pair(k, v) = it {
                        self.gen_expr_ro(k, frame)?;
                        self.gen_expr_ro(v, frame)?;
                        if owns {
                            self.maybe_retain(v, frame); // map owns a value reference
                        }
                        self.emit(&format!("\tmovq ${}, %r8", self.value_flag(v, frame)));
                        self.emit(&format!("\tmovq ${}, %rdx", self.value_flag(k, frame)));
                        self.emit("\tpop %rcx"); // value
                        self.emit("\tpop %rsi"); // key
                        self.emit("\tmov (%rsp), %rdi"); // map
                        self.emit("\tcall flint_hashmap_put");
                    }
                }
                Ok(Ty::HashMap)
            }
        }
    }

    /// Build a list/queue/set from bare element expressions, leaving the new
    /// collection on the expression stack (for `l = [1, 2]` style assigns).
    pub(crate) fn gen_coll_from_elems(
        &mut self,
        kind: CollKind,
        elems: &[Expr],
        ety: Option<Ty>,
        frame: &mut Frame,
    ) -> CompileResult<Ty> {
        let (new_fn, add_fn, flagged, ty) = match kind {
            CollKind::List => ("flint_list_new", "flint_list_add", false, Ty::List),
            CollKind::Queue => ("flint_queue_new", "flint_queue_push", false, Ty::Queue),
            CollKind::Set => ("flint_hashset_new", "flint_hashset_add", true, Ty::HashSet),
            CollKind::Map => {
                return Err(CompileError::new(
                    Span::new(0, 0),
                    "hashmap literals need 'key: value' pairs",
                ))
            }
        };
        let owns = Self::is_obj_elem(ety.as_ref());
        self.emit(&format!("\tmovq ${}, %rdi", elems.len() as i64));
        self.emit_elem_release(ety.as_ref());
        self.emit(&format!("\tcall {}", new_fn));
        self.emit("\tpush %rax");
        for e in elems {
            self.gen_expr_ro(e, frame)?;
            if owns {
                self.maybe_retain(e, frame); // collection owns a reference
            }
            if flagged {
                self.emit(&format!("\tmovq ${}, %rdx", self.value_flag(e, frame)));
            }
            self.emit("\tpop %rsi"); // value
            self.emit("\tmov (%rsp), %rdi"); // collection
            self.emit(&format!("\tcall {}", add_fn));
        }
        Ok(ty)
    }

    /// Store the collection left on the expression stack into `target`,
    /// releasing any collection the target previously held.
    pub(crate) fn store_coll(&mut self, target: &Expr, frame: &mut Frame) -> CompileResult<()> {
        self.emit("\tpop %r10"); // new collection
        self.emit_lvalue_addr(target, frame)?; // pushes the address
        self.emit("\tpop %rdx"); // %rdx = address
        if let Some(k) = self.assign_target_coll(target, frame)? {
            self.emit("\tmovq (%rdx), %r11");
            self.emit("\tmov %r11, %rdi");
            self.emit(&format!("\tcall {}", coll_release_name(k)));
        }
        self.emit("\tmov %r10, (%rdx)");
        Ok(())
    }
}
