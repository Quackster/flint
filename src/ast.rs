use crate::span::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub structs: Vec<ClassDef>,
    pub funcs: Vec<FuncDef>,
    pub interfaces: Vec<InterfaceDef>,
    pub enums: Vec<EnumDef>,
    /// Global import list, collected from `import` statements across all
    /// input files. Each entry is a fully qualified name (`com.other.Foo`)
    /// or a wildcard (`com.other.*`).
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vis {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accessor {
    None,
    Get,
    Set,
    GetSet,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub span: Span,
    pub name: String,
    pub ty: Ty,
    pub vis: Vis,
    pub accessor: Accessor,
    pub is_static: bool,
}

#[derive(Debug, Clone)]
pub struct MethodDef {
    pub span: Span,
    pub name: String,
    pub vis: Vis,
    pub is_static: bool,
    /// `async` method: a call runs the body on a worker thread and evaluates
    /// to a std.Task (join() for the result); the body itself is compiled
    /// as an ordinary function.
    pub is_async: bool,
    pub is_ctor: bool,
    /// Abstract method: declared with `;` and no body. Only allowed in
    /// abstract classes and interfaces; a concrete class must override it.
    pub is_abstract: bool,
    pub type_params: Vec<String>, // v1: always empty (no method-level generics)
    pub params: Vec<(String, Option<Ty>, Span)>,
    pub ret: Option<Ty>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub span: Span,
    /// Package path (dots, no leading dot). Empty = default package.
    pub package: String,
    pub name: String,
    pub type_params: Vec<String>, // e.g. `class Vessel<T>` -> ["T"]; non-empty = generic template
    /// Parent class name (for `extends`); None for a root class.
    pub extends: Option<String>,
    /// Interface names (for `implements`).
    pub implements: Vec<String>,
    /// `abstract class`: cannot be instantiated; may declare abstract methods.
    pub is_abstract: bool,
    pub fields: Vec<FieldDef>,
    pub methods: Vec<MethodDef>,
}

/// An interface: a set of abstract method signatures. v1: no fields, no
/// generic interfaces, no interface inheritance.
#[derive(Debug, Clone)]
pub struct InterfaceDef {
    pub span: Span,
    /// Package path (dots, no leading dot). Empty = default package.
    pub package: String,
    pub name: String,
    pub methods: Vec<MethodDef>, // all abstract
}

/// An enum: a set of named integer constants. v1: no associated data.
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub span: Span,
    pub package: String,
    pub name: String,
    /// (variant_name, value). Values are sequential when not explicitly set.
    pub variants: Vec<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int, // all int keywords collapse to a 64-bit integer in v1
    Bool,
    Ptr(Option<Box<Ty>>), // *T: Some(pointee) when written in source, None =
                         // untyped (null, alloc, @fn; conforms to any *T)
    Str, // NUL-terminated string (a pointer)
    Array, // `T a[]` / `T a[][]`: pointer to 8-byte array slots (v1: rank untracked)
    Void,
    Struct(usize), // index into Program::structs
    Interface(usize), // index into Program::interfaces (a "type" that any
                     // implementing object satisfies; used in decls, casts,
                     // and `instanceof`)
    Enum(usize), // index into Program::enums (a 64-bit integer value)
    // generic (pre-monomorphization only; resolved to concrete types by the
    // middle/mono pass, so the backend never sees these)
    Param(String), // a type-parameter reference, e.g. `T` in `class Vessel<T>`
    Inst(usize, Vec<Ty>), // instantiated generic class: class index + type args
}

#[derive(Debug, Clone)]
pub struct FuncDef {
    pub span: Span,
    /// Package path (dots, no leading dot). Empty = default package.
    pub package: String,
    pub name: String,
    pub type_params: Vec<String>, // e.g. `T identity<T>(T x)` -> ["T"]; non-empty = generic template
    /// `async` function: a call runs the body on a worker thread and
    /// evaluates to a std.Task (join() for the result); the body itself is
    /// compiled as an ordinary function.
    pub is_async: bool,
    pub params: Vec<(String, Option<Ty>, Span)>, // name, ty, span
    pub ret: Option<Ty>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Decl {
        span: Span,
        name: String,
        ty: Option<Ty>,
        value: Expr,
    },
    Assign {
        span: Span,
        target: Expr,
        value: Expr,
    },
    ExprStmt {
        span: Span,
        expr: Expr,
    },
    If {
        span: Span,
        cond: Box<Expr>,
        then: Box<Block>,
        else_opt: Option<Box<Block>>,
    },
    While {
        span: Span,
        /// Loop label for `break name` / `continue name`.
        label: Option<String>,
        cond: Box<Expr>,
        body: Box<Block>,
    },
    For {
        span: Span,
        /// Loop label for `break name` / `continue name`.
        label: Option<String>,
        init: Option<Box<Stmt>>,
        cond: Option<Box<Expr>>,
        update: Option<Box<Stmt>>,
        body: Box<Block>,
    },
    Return {
        span: Span,
        value: Option<Box<Expr>>,
    },
    Break {
        span: Span,
        label: Option<String>,
    },
    Continue {
        span: Span,
        label: Option<String>,
    },
    // `x op= e` (op is Add/Sub/Mul/Div); target must be an lvalue.
    CompoundAssign {
        span: Span,
        target: Expr,
        op: BinOp,
        value: Expr,
    },
    // `switch (t) { case v: ... default: ... }` - int-constant cases, fallthrough allowed.
    Switch {
        span: Span,
        target: Box<Expr>,
        cases: Vec<(i64, Block)>,
        default: Option<Box<Block>>,
    },
    // `throw expr;` - stores expr in the global exception slot and returns.
    Throw {
        span: Span,
        value: Box<Expr>,
    },
    // `try { ... } catch (T var) { ... }` - simple exception handling.
    TryCatch {
        span: Span,
        try_block: Box<Block>,
        catch_type: Ty,
        catch_var: String,
        catch_block: Box<Block>,
        /// Optional `finally { ... }` block; runs on normal completion, on a
        /// caught exception, and on a `return` from the try/catch blocks.
        finally: Option<Box<Block>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    /// `&&` — logical and with short-circuit evaluation.
    And,
    /// `||` — logical or with short-circuit evaluation.
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Deref,  // *
    Addr,   // &
    FnAddr, // @ (function address)
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int {
        span: Span,
        value: i64,
    },
    Bool {
        span: Span,
        value: bool,
    },
    Str {
        span: Span,
        value: String,
    },
    Ident {
        span: Span,
        name: String,
    },
    This {
        span: Span,
    },
    // callee is a dotted path (e.g. ["sys", "write"] or ["printi"]).
    Call {
        span: Span,
        callee: Vec<String>,
        type_args: Vec<Ty>, // explicit type args, e.g. `identity<int>(5)`; empty = infer
        args: Vec<Expr>,
    },
    MethodCall {
        span: Span,
        base: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    BinOp {
        span: Span,
        op: BinOp,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    UnOp {
        span: Span,
        op: UnOp,
        e: Box<Expr>,
    },
    // `++`/`--` on an lvalue; `inc` selects ++/--, `pre` prefix vs postfix.
    IncrDecr {
        span: Span,
        e: Box<Expr>,
        inc: bool,
        pre: bool,
    },
    // value is an expression; `is_addr` means `&expr`.
    AddrOf {
        span: Span,
        e: Box<Expr>,
    },
    Deref {
        span: Span,
        e: Box<Expr>,
    },
    FnAddr {
        span: Span,
        e: Box<Expr>,
    },
    Index {
        span: Span,
        base: Box<Expr>,
        idx: Box<Expr>,
    },
    Field {
        span: Span,
        base: Box<Expr>,
        name: String,
    },
    StructLit {
        span: Span,
        name: String,
        type_args: Vec<Ty>, // explicit type args, e.g. `new Vessel<int>(5)`; empty = infer
        fields: Vec<(String, Expr)>,
    },
    ArrayLit {
        span: Span,
        elems: Vec<Expr>,
    },
    Null {
        span: Span,
    },
    // `c ? a : b`
    Cond {
        span: Span,
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    // `await e`: block until the task finishes; evaluate to the value the
    // task produced. `e` is a call to an `async` function/method (the call
    // runs on a worker thread and the await waits for it) or an expression
    // of type `std.Task` (the await blocks on its `join`). A void function
    // yields 0; awaiting a bare `Task` yields an int.
    Await {
        span: Span,
        e: Box<Expr>,
    },
    // `super` as a receiver: `super.field` / `super.method(args)`.
    SuperBase {
        span: Span,
    },
    // `super(args)`: invoke the parent class constructor on `this`.
    SuperCall {
        span: Span,
        args: Vec<Expr>,
    },
    // `(Type) e`: up/downcast a class value. Upcast is a no-op; downcast is
    // unchecked in v1 (trusts the programmer).
    Cast {
        span: Span,
        ty: Ty,
        e: Box<Expr>,
    },
    // `e instanceof Type`: true when `e`'s dynamic type is Type or a subtype.
    Instanceof {
        span: Span,
        e: Box<Expr>,
        ty: Ty,
    },
    // `EnumName.Variant`: a compile-time integer constant.
    EnumVariant {
        span: Span,
        enum_name: String,
        variant: String,
    },
    // A lambda expression, Java-style: `int x -> boolean { ... }`,
    // `(int a, int b) -> int { ... }`, or `() -> void { ... }`. The middle
    // pass lifts it into a generated top-level function (whose first
    // parameter is the `*int` capture block) and replaces it with a Closure.
    Lambda {
        span: Span,
        params: Vec<(String, Option<Ty>, Span)>,
        ret: Option<Ty>,
        body: Block,
    },
    // The value of a lambda: a pointer to a fresh heap block
    // `[fn_ptr, cap0, cap1, ...]`. `captures` are evaluated (by value) when
    // the block is created; the generated function is `fn_name` (already
    // mangled). Calling convention: `fn(ctx, arg1, arg2)`.
    Closure {
        span: Span,
        fn_name: String,
        captures: Vec<Expr>,
    },
}
