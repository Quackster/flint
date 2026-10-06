// Ownership checking: compile-time memory safety like Rust (Ch 4.1).
//
// Rules enforced (see https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html):
//   1. Each value has an owner (the variable or field that holds it).
//   2. There is only one owner at a time (`T x = y`, `x = y`, `return y`,
//      and `obj.field = y` *move* class objects, `*T` buffers, strings,
//      and arrays; the old name becomes invalid).
//   3. When the owner goes out of scope, the value is dropped (the backend
//      releases class values, strings, and arrays at scope end or on
//      overwrite — a real `munmap`; raw `*T` pointers never own and are
//      never freed).
//
// What is tracked:
//   - `Struct` / `Interface` (class objects), `*T` buffers, `string`, and
//     arrays: moves on `T x = y`, `x = y`, `return y`, and field stores.
//     Use-after-move is a compile error. Clone explicitly with `x.move()`
//     (objects: same address, extra reference — `mem.retainVal(x)` also
//     works; strings/arrays: plain shared alias).
//   - Calls *borrow*, so `x.foo()`, `f(x)`, `strlen(s)`,
//     `str.concat(a, b)`, and `sys.*` never move their arguments.
//     (A `new X(...)` passed straight into a call has no named owner to
//     release it — see the `obj_coll` example: hold it in a local first.)
//   - `int` / `bool` / enum: `Copy` (never move, always usable).
//   - `this` (the method receiver): borrowed like Rust `&self`, never moves.
//     Field reads (`x.f`, `buf[i]`) borrow.
//
// Explicit clones (borrow the source, return a fresh owner):
//   - `x.move()` on any object, string, or array (raw `*T` buffers have no
//     `move()` — they never own and are never freed; a user-defined
//     `move` method wins over this builtin).
//   - `str.copy(s)` (deep copy; `str.concat`/`substring`/etc. also return
//     fresh values while borrowing their inputs).
//   - Buffers come only from typed initialization (`int buf[n]`,
//     `string s[n]`), array/string literals, and calls that return fresh
//     values; there is no implicit aliasing — `*int q = p` moves `p`.
//
// Fields are not tracked (like Rust `unsafe` for raw slot arrays): the
// collections (`src/stdlib/coll.flint`) store elements as raw `int` slots
// with manual `Mem.retain` / `Mem.release` and are therefore exempt — they
// are the `unsafe` core, while application code gets checked moves.
//
// Leaks are allowed (like `Rc` cycles / `mem::forget` in Rust): an object
// cycle, or an unnamed call result nobody owns (e.g. a `new` passed
// straight into a call), simply reclaims at process exit. What is
// *forbidden* is using a value after its owner moved or dropped it.
use crate::ast::{Block, Expr, Program, Stmt, Ty};
use crate::error::{CompileError, CompileResult};
use crate::span::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Owned,
    Moved,
}

#[derive(Debug, Clone)]
struct Var {
    ty: Ty,
    state: State,
    /// Span of the move/free that invalidated this variable (for diagnostics).
    moved_at: Option<Span>,
}

// Types with move semantics (single owner): class objects, `string`, and
// arrays (each dropped via its release helper at scope end or on
// overwrite); raw `*T` pointers never own and are never freed. A move
// transfers the single live name — sharing one value under two live names
// needs an explicit `x.move()` clone (on objects a retain, on strings and
// arrays a plain alias, same address, both stay valid; strings that must be
// independent need `str.copy(s)`). Calls still borrow, so `strlen(s)`,
// `str.concat(a, b)`, and the desugared `for (x : c)` temporaries keep
// working without clones.
fn is_owned_ty(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Struct(_)
            | Ty::Interface(_)
            | Ty::Ptr(_)
            | Ty::Str
            | Ty::Array
            | Ty::ByteArray
            | Ty::ShortArray
    )
}

struct Checker<'p> {
    prog: &'p Program,
    /// Scope stack for shadowing; index 0 is the function scope.
    scopes: Vec<HashMap<String, Var>>,
}

impl<'p> Checker<'p> {
    fn new(prog: &'p Program) -> Self {
        Self {
            prog,
            scopes: vec![HashMap::new()],
        }
    }

    fn declare(&mut self, name: String, ty: Ty) {
        let v = Var {
            state: State::Owned,
            ty,
            moved_at: None,
        };
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name, v);
        }
    }

    fn find_mut(&mut self, name: &str) -> Option<&mut Var> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(v) = scope.get_mut(name) {
                return Some(v);
            }
        }
        None
    }

    fn find(&self, name: &str) -> Option<&Var> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v);
            }
        }
        None
    }

    fn var_ty(&self, name: &str) -> Option<Ty> {
        self.find(name).map(|v| v.ty.clone())
    }

    fn is_owned_var(&self, name: &str) -> bool {
        self.var_ty(name).map(|t| is_owned_ty(&t)).unwrap_or(false)
    }

    fn use_borrow(&mut self, name: &str, span: Span) -> CompileResult<()> {
        if name == "this" {
            return Ok(());
        }
        // Unknown names (globals, class names, statics) are not tracked.
        let Some(v) = self.find(name) else {
            return Ok(());
        };
        if !is_owned_ty(&v.ty) {
            return Ok(());
        }
        if v.state == State::Moved {
            return Err(CompileError::new(
                span,
                format!(
                    "use of moved value `{}` (each value has a single owner; `{}` was moved or freed here — clone with `{}.move()` (shared) or `str.copy({})` (independent string); see https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)",
                    name, name, name, name
                ),
            ));
        }
        Ok(())
    }

    fn use_move(&mut self, name: &str, span: Span) -> CompileResult<()> {
        if name == "this" {
            return Err(CompileError::new(
                span,
                "cannot move `this` (the method receiver is borrowed like `&self`; it cannot be moved)",
            ));
        }
        let Some(v) = self.find_mut(name) else {
            return Ok(());
        };
        if !is_owned_ty(&v.ty) {
            return Ok(());
        }
        if v.state == State::Moved {
            // Distinguish free-related double use when possible via message.
            return Err(CompileError::new(
                span,
                format!(
                    "use of moved value `{}` (it was already moved or freed; each value has a single owner — clone with `{}.move()` (shared) or `str.copy({})` (independent string); see https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)",
                    name, name, name
                ),
            ));
        }
        v.state = State::Moved;
        v.moved_at = Some(span);
        Ok(())
    }

    fn reinit(&mut self, name: &str, ty: Ty) {
        // Reassigning a (possibly moved) variable makes it owned again.
        if let Some(v) = self.find_mut(name) {
            v.ty = ty;
            v.state = State::Owned;
            v.moved_at = None;
        }
    }

    fn snapshot(&self) -> Vec<HashMap<String, Var>> {
        self.scopes.clone()
    }

    fn restore(&mut self, snap: &Vec<HashMap<String, Var>>) {
        self.scopes = snap.clone();
    }

    /// Merge `other` (a branch end) into `self` (the other branch end):
    /// a variable is Moved after the join when it is Moved in either
    /// branch (conservative, like Rust's join over control flow).
    fn merge_branch(&mut self, other: &Vec<HashMap<String, Var>>) {
        // Only merge variables present in both (outer scopes); branch-locals
        // declared inside the branch scopes are dropped with those scopes.
        // Since branches share the same scope-stack shape (blocks push/pop
        // symmetrically), merge per-scope by name.
        for (scope_self, scope_other) in self.scopes.iter_mut().zip(other.iter()) {
            for (name, v_other) in scope_other {
                if let Some(v_self) = scope_self.get_mut(name) {
                    if v_other.state == State::Moved || v_self.state == State::Moved {
                        // Keep the earliest move span for diagnostics.
                        if v_self.state != State::Moved {
                            v_self.state = State::Moved;
                            v_self.moved_at = v_other.moved_at;
                        }
                    }
                }
            }
            // Variables declared only in one branch (e.g. shadowing inside
            // the branch block) stay local to that block and are already
            // popped; nothing to do here.
        }
    }
}

/// True for builtin calls (they borrow all arguments and return fresh
/// values). Matches `backend/codegen/builtin.rs`.
fn is_builtin_callee(callee: &[String]) -> bool {
    let name = callee.join(".");
    matches!(
        name.as_str(),
        "sys.exit"
            | "exit"
            | "sys.write"
            | "write"
            | "sys.read"
            | "read"
            | "sys.open"
            | "open"
            | "sys.close"
            | "close"
            | "sys.brk"
            | "brk"
            | "sys.socket"
            | "socket"
            | "sys.bind"
            | "bind"
            | "sys.listen"
            | "listen"
            | "sys.accept"
            | "accept"
            | "sys.connect"
            | "connect"
            | "sys.sockaddr"
            | "sockaddr"
            | "sys.alloc"
            | "alloc"
            | "mem.alloc"
            | "sys.free"
            | "free"
            | "mem.free"
            | "mem.retain"
            | "mem.release"
            | "memcpy"
            | "mem.memcpy"
            | "strlen"
            | "str.len"
            | "print"
            | "io.print"
            | "println"
            | "io.println"
            | "atoi"
            | "conv.atoi"
            | "len"
            | "str.cmp"
            | "strcmp"
            | "str.copy"
            | "strcpy"
            | "str.concat"
            | "concat"
            | "str.concati"
            | "concati"
            | "str.itoa"
            | "itoa"
            | "time.millis"
            | "time.now"
            | "time"
            | "rand.next"
            | "rand"
            | "rand.range"
            | "env.get"
            | "getenv"
            | "str.substring"
            | "str.indexOf"
            | "str.replace"
            | "b64.encode"
            | "b64.decode"
            | "log.info"
            | "log.warn"
            | "log.error"
            | "log.debug"
            | "sys.select"
            | "sys.poll"
            | "sys.epollCreate1"
            | "sys.epollCtl"
            | "sys.epollWait"
            | "sys.clone"
            | "sys.futex"
            | "sys.syscall"
            | "sys.mmap"
            | "sys.munmap"
            | "math.fadd"
            | "fadd"
            | "math.fsub"
            | "fsub"
            | "math.fmul"
            | "fmul"
            | "math.fdiv"
            | "fdiv"
            | "math.ftoi"
            | "ftoi"
            | "math.itof"
            | "itof"
            | "math.fcmp"
            | "fcmp"
            | "sys.threadCreate"
            | "sys.threadJoin"
            | "sys.mutexLock"
            | "sys.mutexUnlock"
            | "sys.atomicCas"
            | "sys.nanosleep"
            | "json.get"
            | "json.geti"
            | "str.format"
            | "mem.retainVal"
            | "mem.retainStr"
            | "mem.releaseStr"
            | "sys.fnCall2"
            | "fnCall2"
            | "sys.fnCall3"
            | "fnCall3"
            | "assert"
            | "io.assert"
            | "panic"
            | "io.panic"
    )
}

fn is_free_call(callee: &[String]) -> bool {
    let name = callee.join(".");
    matches!(name.as_str(), "free" | "sys.free" | "mem.free")
}

fn is_free_method(method: &str) -> bool {
    matches!(method, "freeInt" | "freeByte")
}

impl<'p> Checker<'p> {
    fn check_expr_borrow(&mut self, e: &Expr) -> CompileResult<()> {
        match e {
            Expr::Ident { name, span } => self.use_borrow(name, *span),
            Expr::This { .. } => Ok(()),
            Expr::SuperBase { .. } => Ok(()),
            // Calls (even nested in a borrow position) still move their
            // class-typed ident args — e.g. `print(sum(a))` moves `a`.
            Expr::Call { callee, args, span, .. } => self.check_call(callee, args, *span),
            Expr::MethodCall { base, method, args, span, .. } => {
                self.check_method_call(base, method, args, *span)
            }
            Expr::BinOp { l, r, .. } => {
                self.check_expr_value(l)?;
                self.check_expr_value(r)?;
                Ok(())
            }
            Expr::UnOp { e: inner, .. } => self.check_expr_value(inner),
            Expr::IncrDecr { e: inner, .. } => self.check_expr_borrow(inner),
            Expr::AddrOf { e: inner, .. } => self.check_expr_borrow(inner),
            Expr::Deref { e: inner, .. } => self.check_expr_value(inner),
            Expr::FnAddr { .. } => Ok(()),
            Expr::Index { base, idx, .. } => {
                self.check_expr_value(base)?;
                self.check_expr_value(idx)?;
                Ok(())
            }
            Expr::Field { base, .. } => self.check_expr_value(base),
            Expr::StructLit { fields, .. } => {
                for (_, v) in fields {
                    // Constructor / field-init args: class args move, the
                    // rest borrow (mirrors call-arg handling below).
                    self.check_structlit_arg(v)?;
                }
                Ok(())
            }
            Expr::ArrayLit { elems, .. } => {
                for v in elems {
                    self.check_expr_borrow(v)?;
                }
                Ok(())
            }
            // A fresh owned buffer (moved into its slot, never from one).
            Expr::SizedNew { size, .. } => self.check_expr_borrow(size),
            Expr::Cond { cond, then, els, .. } => {
                self.check_expr_value(cond)?;
                // Ternary moves like an if: merge the two branches.
                let snap = self.snapshot();
                self.check_expr_value(then)?;
                let then_end = self.snapshot();
                self.restore(&snap);
                self.check_expr_value(els)?;
                let els_end = self.snapshot();
                self.restore(&then_end);
                self.merge_branch(&els_end);
                Ok(())
            }
            Expr::Await { e: inner, .. } => self.check_expr_value(inner),
            Expr::SuperCall { args, .. } => {
                for a in args {
                    self.check_call_arg(a)?;
                }
                Ok(())
            }
            Expr::Cast { e: inner, .. } => self.check_expr_value(inner),
            Expr::Instanceof { e: inner, .. } => self.check_expr_value(inner),
            Expr::Closure { captures, .. } => {
                for c in captures {
                    self.check_expr_borrow(c)?;
                }
                Ok(())
            }
            Expr::Lambda { .. } => Ok(()),
            // Interpolated sub-expressions are read by value (the result
            // string is fresh); a part never moves its own operands.
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if !matches!(p, Expr::Str { .. }) {
                        self.check_expr_value(p)?;
                    }
                }
                Ok(())
            }
            // `base?.rest`: the base is null-tested and used as receiver
            // (both borrows); the rest is evaluated for value (nested moves
            // inside it apply, but the base itself never moves).
            Expr::OptChain { base, rest, .. } => {
                self.check_expr_borrow(base)?;
                self.check_expr_value(rest)?;
                Ok(())
            }
            // The base of the enclosing optional chain: borrowed, never moved.
            Expr::OptRef { .. } => Ok(()),
            // `l ?? r`: the result reuses whichever side is non-null; the
            // sides are borrows (the backend takes the extra reference).
            Expr::Coalesce { l, r, .. } => {
                self.check_expr_borrow(l)?;
                self.check_expr_borrow(r)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Check a constructor / struct-literal argument: always borrows.
    /// Calls borrow their arguments (the callee clones via `retain` when it
    /// needs to keep them, like Rust `&T`); moves happen only for plain
    /// bindings (`T x = y`), field stores, `return`, and `free`.
    fn check_structlit_arg(&mut self, e: &Expr) -> CompileResult<()> {
        if let Expr::Ident { name, span } = e {
            return self.use_borrow(name, *span);
        }
        // Non-ident args: recurse; nested class moves inside (e.g.
        // `new Pair(makeBox(), 1)`) are handled by the nested call checks.
        match e {
            Expr::Call { callee, args, span, .. } => self.check_call(callee, args, *span),
            Expr::MethodCall { base, method, args, span, .. } => {
                self.check_method_call(base, method, args, *span)
            }
            _ => self.check_expr_borrow(e),
        }
    }

    /// Check a call argument in a user call: always borrows (see above).
    fn check_call_arg(&mut self, e: &Expr) -> CompileResult<()> {
        if let Expr::Ident { name, span } = e {
            return self.use_borrow(name, *span);
        }
        match e {
            Expr::Call { callee, args, span, .. } => self.check_call(callee, args, *span),
            Expr::MethodCall { base, method, args, span, .. } => {
                self.check_method_call(base, method, args, *span)
            }
            _ => self.check_expr_borrow(e),
        }
    }

    fn check_call(
        &mut self,
        callee: &[String],
        args: &[Expr],
        span: Span,
    ) -> CompileResult<()> {
        let _ = span;
        if is_free_call(callee) {
            // `free(p)`: consume `p` (use-after-free / double-free become
            // use-of-moved errors).
            if let Some(a) = args.first() {
                if let Expr::Ident { name, span } = a {
                    // `free` on a Copy (int) is a no-op for tracking; on an
                    // owned pointer it moves to freed.
                    if self.is_owned_var(name) {
                        self.use_move(name, *span)?;
                    }
                    // Check any other args (none) as borrow for completeness.
                    return Ok(());
                }
                return self.check_expr_borrow(a);
            }
            return Ok(());
        }
        if is_builtin_callee(callee) {
            for a in args {
                self.check_expr_borrow(a)?;
            }
            return Ok(());
        }
        // User function: class-typed ident args move, the rest borrow.
        for a in args {
            self.check_call_arg(a)?;
        }
        Ok(())
    }

    fn check_method_call(
        &mut self,
        base: &Expr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> CompileResult<()> {
        let _ = span;
        // Static `Mem.freeInt(p)` / `Mem.freeByte(p)`: consume `p`.
        if is_free_method(method) {
            if let Expr::Ident { name, .. } = base {
                if name == "Mem" || name == "std.Mem" {
                    if let Some(a) = args.first() {
                        if let Expr::Ident { name, span } = a {
                            if self.is_owned_var(name) {
                                self.use_move(name, *span)?;
                            }
                            return Ok(());
                        }
                        return self.check_expr_borrow(a);
                    }
                    return Ok(());
                }
            }
        }
        // The receiver always borrows (like `&self`); it never moves.
        self.check_expr_borrow(base)?;
        // Class-typed ident args move; `*T` / `string` / Copy args borrow.
        // (Builtins are Calls, not MethodCalls, except `str.*`? `str.*`
        // go through Call with dotted callee, handled above. `sys.*`
        // likewise. So here: user + stdlib methods.)
        for a in args {
            // `.move()`-style clones take no args (base borrows).
            self.check_call_arg(a)?;
        }
        Ok(())
    }

    fn check_expr_value(&mut self, e: &Expr) -> CompileResult<()> {
        // A value in a non-move position: evaluate it, moving class args
        // of nested calls but never moving a top-level ident itself.
        match e {
            Expr::Ident { name, span } => self.use_borrow(name, *span),
            Expr::Call { callee, args, span, .. } => self.check_call(callee, args, *span),
            Expr::MethodCall { base, method, args, span, .. } => {
                self.check_method_call(base, method, args, *span)
            }
            _ => self.check_expr_borrow(e),
        }
    }

    /// Check the RHS of a move position (`Decl` / `Assign` / `return` /
    /// field store): a bare owned ident moves; anything else is evaluated
    /// for its nested moves and yields a fresh/borrowed value.
    fn check_move_rhs(&mut self, e: &Expr) -> CompileResult<()> {
        if let Expr::Ident { name, span } = e {
            if name == "this" {
                return self.use_borrow(name, *span);
            }
            if self.is_owned_var(name) {
                return self.use_move(name, *span);
            }
            return Ok(());
        }
        // `&x` borrows even in a move position (`*int p = &x` aliases the
        // local; the existing dangling-return check covers `return &x`).
        if let Expr::AddrOf { e: inner, .. } = e {
            return self.check_expr_borrow(inner);
        }
        self.check_expr_value(e)
    }

    /// Check the value of a tuple destructure (`(a, b) = value`): a tuple
    /// literal moves its bare owned elements into the fresh slots; anything
    /// else (a call) is checked as a plain move RHS.
    fn check_tuple_rhs(&mut self, e: &Expr) -> CompileResult<()> {
        if let Expr::Tuple { elems, .. } = e {
            for el in elems {
                self.check_move_rhs(el)?;
            }
            return Ok(());
        }
        self.check_move_rhs(e)
    }

    fn check_block(&mut self, b: &Block) -> CompileResult<bool> {
        self.scopes.push(HashMap::new());
        let mut diverged = false;
        for s in &b.stmts {
            if diverged {
                break;
            }
            if self.check_stmt(s)? {
                diverged = true;
            }
        }
        self.scopes.pop();
        Ok(diverged)
    }

    fn check_block_inline(&mut self, b: &Block) -> CompileResult<bool> {
        // For function bodies the scope is already pushed by the caller.
        let mut diverged = false;
        for s in &b.stmts {
            if diverged {
                break;
            }
            if self.check_stmt(s)? {
                diverged = true;
            }
        }
        Ok(diverged)
    }

    /// Check one statement. Returns true when it diverges (never falls
    /// through: `return` / `throw`).
    fn check_stmt(&mut self, s: &Stmt) -> CompileResult<bool> {
        match s {
            Stmt::Decl { name, ty, value, .. } => {
                // Desugared for-each bases (`.rf_base_N = target`) borrow the
                // target (read-only iteration, like Rust `&`), so the same
                // collection can be iterated multiple times.
                if name.starts_with(".rf_base_") {
                    self.check_expr_value(value)?;
                } else {
                    self.check_move_rhs(value)?;
                }
                // Declare (or shadow) the name in the current scope.
                let t = ty.clone().unwrap_or(Ty::Int);
                if let Some(top) = self.scopes.last_mut() {
                    top.insert(
                        name.clone(),
                        Var {
                            ty: t,
                            state: State::Owned,
                            moved_at: None,
                        },
                    );
                }
                Ok(false)
            }
            Stmt::Assign { target, value, .. } => {
                // Index stores (`buf[i] = v`) never move `v` (raw slots are
                // manually managed, like `unsafe`); field stores move owned
                // class/pointer values into the field; plain ident targets
                // reinit the variable.
                match target {
                    Expr::Index { base, idx, .. } => {
                        self.check_expr_value(value)?;
                        self.check_expr_borrow(base)?;
                        self.check_expr_borrow(idx)?;
                        Ok(false)
                    }
                    Expr::Field { base, .. } => {
                        // Field stores move owned class/pointer values into
                        // the field (single owner = the field); strings and
                        // arrays borrow (they leak, so aliasing is safe).
                        if let Expr::Ident { name, span } = value {
                            if self.is_owned_var(name) {
                                self.use_move(name, *span)?;
                            } else {
                                self.use_borrow(name, *span)?;
                            }
                        } else if let Expr::AddrOf { .. } = value {
                            self.check_expr_borrow(value)?;
                        } else {
                            self.check_expr_value(value)?;
                        }
                        self.check_expr_borrow(base)?;
                        Ok(false)
                    }
                    Expr::Ident { name, .. } => {
                        // `x = <rhs>`: rhs moves (if bare owned ident),
                        // then `x` is reinitialized to owned.
                        let rhs_ty = self.infer_rhs_ty(value);
                        self.check_move_rhs(value)?;
                        // Reinit the target when it is a tracked variable.
                        // Keep its declared type when known; otherwise take
                        // the RHS type when it is owned.
                        if self.find(name).is_some() {
                            let cur = self.var_ty(name).unwrap_or(Ty::Int);
                            let nt = if is_owned_ty(&cur) {
                                cur
                            } else {
                                rhs_ty.unwrap_or(cur)
                            };
                            self.reinit(name, nt);
                        } else {
                            // Assigning to an unknown name (global/static):
                            // just check the value, no state to update.
                        }
                        // Also check the target itself is not a moved base?
                        // (`x` as a place is fine even when moved: this is
                        // the reinit.)
                        Ok(false)
                    }
                    _ => {
                        self.check_expr_value(value)?;
                        self.check_expr_borrow(target)?;
                        Ok(false)
                    }
                }
            }
            Stmt::TupleDecl { fields, value, .. } => {
                self.check_tuple_rhs(value)?;
                for (ty, name, _) in fields {
                    if let Some(top) = self.scopes.last_mut() {
                        top.insert(
                            name.clone(),
                            Var {
                                ty: ty.clone(),
                                state: State::Owned,
                                moved_at: None,
                            },
                        );
                    }
                }
                Ok(false)
            }
            Stmt::TupleVar { names, tys, value, .. } => {
                self.check_tuple_rhs(value)?;
                for ((name, _), ty) in names.iter().zip(tys.iter()) {
                    if let Some(top) = self.scopes.last_mut() {
                        top.insert(
                            name.clone(),
                            Var {
                                ty: ty.clone(),
                                state: State::Owned,
                                moved_at: None,
                            },
                        );
                    }
                }
                Ok(false)
            }
            Stmt::TupleAssign { targets, value, .. } => {
                self.check_tuple_rhs(value)?;
                for t in targets {
                    if let Expr::Ident { name, .. } = t {
                        if self.find(name).is_some() {
                            let cur = self.var_ty(name).unwrap_or(Ty::Int);
                            self.reinit(name, cur);
                        }
                    }
                }
                Ok(false)
            }
            Stmt::ExprStmt { expr, .. } => {
                // A bare owned ident as a statement drops it (moves).
                if let Expr::Ident { name, span } = expr {
                    if self.is_owned_var(name) {
                        self.use_move(name, *span)?;
                        return Ok(false);
                    }
                    return Ok(false);
                }
                self.check_expr_value(expr)?;
                Ok(false)
            }
            Stmt::If { cond, then, else_opt, .. } => {
                self.check_expr_borrow(cond)?;
                let snap = self.snapshot();
                let then_div = self.check_block(then)?;
                let then_end = self.snapshot();
                self.restore(&snap);
                let els_div = if let Some(eb) = else_opt {
                    self.check_block(eb)?
                } else {
                    false
                };
                if then_div && els_div {
                    // Both diverge: the if diverges; keep either end-state.
                    self.restore(&then_end);
                    return Ok(true);
                } else if then_div {
                    // Then diverges: only the else path falls through.
                    return Ok(false);
                } else if els_div {
                    // Else diverges: only the then path falls through.
                    self.restore(&then_end);
                    return Ok(false);
                } else {
                    let els_end = self.snapshot();
                    // Merge: moved in either branch stays moved.
                    self.restore(&then_end);
                    self.merge_branch(&els_end);
                    return Ok(false);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.check_expr_borrow(cond)?;
                let snap = self.snapshot();
                // Two passes catch moves that are used on the next iteration.
                let _ = self.check_block(body);
                let mid = self.snapshot();
                self.restore(&snap);
                let _ = self.check_block(body);
                let end = self.snapshot();
                // Merge body effects conservatively (a move in the body may
                // have happened), then also merge the first pass.
                self.restore(&mid);
                self.merge_branch(&end);
                // A loop may execute zero times: keep pre-loop ownership for
                // variables not moved on every path? Conservative choice
                // (like Rust): a move inside the body poisons the variable
                // after the loop.
                let _ = snap;
                Ok(false)
            }
            Stmt::For { init, cond, update, body, .. } => {
                // `for` init lives in its own scope.
                self.scopes.push(HashMap::new());
                if let Some(i) = init {
                    let d = self.check_stmt(i)?;
                    if d {
                        self.scopes.pop();
                        return Ok(true);
                    }
                }
                if let Some(c) = cond {
                    self.check_expr_borrow(c)?;
                }
                let snap = self.snapshot();
                let _ = self.check_block(body);
                if let Some(u) = update {
                    let _ = self.check_stmt(u);
                }
                let mid = self.snapshot();
                // Second pass for cross-iteration uses.
                self.restore(&snap);
                let _ = self.check_block(body);
                if let Some(u) = update {
                    let _ = self.check_stmt(u);
                }
                let end = self.snapshot();
                self.restore(&mid);
                self.merge_branch(&end);
                self.scopes.pop();
                Ok(false)
            }
            Stmt::ForEach { target, body, name, ty, .. } => {
                // Desugared by mono; defensive: borrow the target and bind a
                // fresh loop variable borrowing one element.
                self.check_expr_borrow(target)?;
                self.scopes.push(HashMap::new());
                let t = ty.clone().unwrap_or(Ty::Int);
                if let Some(top) = self.scopes.last_mut() {
                    top.insert(
                        name.clone(),
                        Var {
                            ty: t,
                            state: State::Owned,
                            moved_at: None,
                        },
                    );
                }
                let _ = self.check_block(body);
                self.scopes.pop();
                Ok(false)
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    // `return &x` for locals is already rejected by codegen;
                    // other bare owned idents move into the caller. A tuple
                    // return moves each owned element into the caller.
                    if matches!(v.as_ref(), Expr::AddrOf { .. }) {
                        self.check_expr_borrow(v)?;
                    } else if let Expr::Tuple { elems, .. } = v.as_ref() {
                        for el in elems {
                            self.check_move_rhs(el)?;
                        }
                    } else {
                        self.check_move_rhs(v)?;
                    }
                }
                Ok(true)
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => Ok(false),
            Stmt::CompoundAssign { target, value, .. } => {
                self.check_expr_borrow(value)?;
                self.check_expr_borrow(target)?;
                Ok(false)
            }
            Stmt::Switch { target, cases, default, .. } => {
                self.check_expr_borrow(target)?;
                let snap = self.snapshot();
                let mut any_fall = false;
                let mut acc: Option<Vec<HashMap<String, Var>>> = None;
                for (_, b) in cases {
                    self.restore(&snap);
                    let div = self.check_block(b)?;
                    if !div {
                        any_fall = true;
                        let end = self.snapshot();
                        match &mut acc {
                            None => acc = Some(end),
                            Some(a) => {
                                // Merge into accumulator.
                                let mut tmp = Checker {
                                    prog: self.prog,
                                    scopes: a.clone(),
                                };
                                tmp.merge_branch(&end);
                                *a = tmp.scopes;
                            }
                        }
                    }
                }
                if let Some(db) = default {
                    self.restore(&snap);
                    let div = self.check_block(db)?;
                    if !div {
                        any_fall = true;
                        let end = self.snapshot();
                        match &mut acc {
                            None => acc = Some(end),
                            Some(a) => {
                                let mut tmp = Checker {
                                    prog: self.prog,
                                    scopes: a.clone(),
                                };
                                tmp.merge_branch(&end);
                                *a = tmp.scopes;
                            }
                        }
                    }
                } else {
                    // No default: the "no case matches" path falls through
                    // with pre-switch states.
                    any_fall = true;
                    match &mut acc {
                        None => acc = Some(snap.clone()),
                        Some(a) => {
                            let mut tmp = Checker {
                                prog: self.prog,
                                scopes: a.clone(),
                            };
                            tmp.merge_branch(&snap);
                            *a = tmp.scopes;
                        }
                    }
                }
                if let Some(a) = acc {
                    self.restore(&a);
                } else {
                    self.restore(&snap);
                }
                Ok(!any_fall)
            }
            Stmt::Throw { value, .. } => {
                if let Expr::AddrOf { .. } = value.as_ref() {
                    self.check_expr_borrow(value)?;
                } else {
                    self.check_move_rhs(value)?;
                }
                Ok(true)
            }
            Stmt::Defer { expr, .. } => {
                // Evaluated at function exit, while the operands are still
                // alive: check it like a plain expression statement.
                if let Expr::Ident { name, span } = expr.as_ref() {
                    if self.is_owned_var(name) {
                        self.use_move(name, *span)?;
                    }
                    return Ok(false);
                }
                self.check_expr_value(expr)?;
                Ok(false)
            }
            Stmt::TryCatch {
                try_block,
                catch_types,
                catch_var,
                catch_block,
                finally,
                ..
            } => {
                let snap = self.snapshot();
                let try_div = self.check_block(try_block)?;
                let try_end = self.snapshot();
                self.restore(&snap);
                // The catch variable is a fresh owner (the thrown value moves
                // into it), typed by the catch types' least upper bound.
                let vty = crate::backend::layout::lub_types(self.prog, catch_types)
                    .unwrap_or_else(|| catch_types[0].clone());
                self.scopes.push(HashMap::new());
                if let Some(top) = self.scopes.last_mut() {
                    top.insert(
                        catch_var.clone(),
                        Var {
                            ty: vty,
                            state: State::Owned,
                            moved_at: None,
                        },
                    );
                }
                let catch_div = self.check_block_inline(catch_block)?;
                self.scopes.pop();
                let catch_end = self.snapshot();
                if try_div && catch_div {
                    // Both diverge; finally still runs.
                    if let Some(fb) = finally {
                        let _ = self.check_block(fb);
                    }
                    return Ok(true);
                } else if try_div {
                    self.restore(&catch_end);
                } else if catch_div {
                    self.restore(&try_end);
                } else {
                    self.restore(&try_end);
                    self.merge_branch(&catch_end);
                }
                if let Some(fb) = finally {
                    let _ = self.check_block(fb);
                }
                Ok(false)
            }
        }
    }

    fn infer_rhs_ty(&self, e: &Expr) -> Option<Ty> {
        match e {
            Expr::Ident { name, .. } => self.var_ty(name),
            Expr::This { .. } => None,
            Expr::StructLit { name, .. } => self
                .prog
                .structs
                .iter()
                .position(|s| &s.name == name)
                .map(Ty::Struct),
            Expr::Call { .. } => None,
            Expr::MethodCall { .. } => None,
            Expr::Str { .. } => Some(Ty::Str),
            Expr::Null { .. } => Some(Ty::Ptr(None)),
            _ => None,
        }
    }
}

pub fn check(prog: &Program) -> CompileResult<()> {
    // Build once for field-type lookups (currently informational; field
    // stores move based on the RHS variable type).
    for f in &prog.funcs {
        let mut c = Checker::new(prog);
        for (pname, pty, _, _) in &f.params {
            let t = pty.clone().unwrap_or(Ty::Int);
            c.declare(pname.clone(), t);
        }
        c.check_block_inline(&f.body)?;
    }
    for s in &prog.structs {
        // Resolve `this` to this class for borrow checks (never moves).
        for m in &s.methods {
            let mut c = Checker::new(prog);
            if !m.is_static {
                // `this` is present but borrowed; model it as an owned var
                // that `use_move` refuses to move (see the `this` guard).
                if let Some(idx) = prog.structs.iter().position(|x| x.name == s.name) {
                    c.declare("this".to_string(), Ty::Struct(idx));
                }
            }
            for (pname, pty, _, _) in &m.params {
                let t = pty.clone().unwrap_or(Ty::Int);
                c.declare(pname.clone(), t);
            }
            c.check_block_inline(&m.body)?;
        }
    }
    Ok(())
}
