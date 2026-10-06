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
    /// Top-level `const NAME = <value>;` constants, inlined at use sites.
    pub consts: Vec<ConstDef>,
    /// Top-level `type NAME = <type>;` aliases, resolved by the parser.
    pub type_aliases: Vec<TypeAlias>,
}

/// A compile-time constant value (int, bool, or string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstVal {
    Int(i64),
    Bool(bool),
    Str(String),
}

/// `const NAME = <value>;` — top level (scoped by `package`) or a class
/// member (`Class.NAME`). The monomorphizer inlines uses as literals.
#[derive(Debug, Clone)]
pub struct ConstDef {
    pub span: Span,
    /// Package for top-level consts; empty for class members.
    pub package: String,
    pub name: String,
    pub value: ConstVal,
}

/// `type NAME = <type>;` — a top-level type alias.
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub span: Span,
    pub package: String,
    pub name: String,
    pub ty: Ty,
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
    /// (name, ty, span, default); the default (only on trailing params) is
    /// evaluated in the caller's context when the argument is omitted.
    pub params: Vec<(String, Option<Ty>, Span, Option<Expr>)>,
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
    /// `const NAME = <value>;` class members, referenced as `Class.NAME`.
    pub consts: Vec<ConstDef>,
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
    ByteArray, // `byte b[n]` / `char c[n]`: n contiguous 1-byte elements
    ShortArray, // `short s[n]`: n contiguous 2-byte elements (little-endian)
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
    Alias(String), // `type NAME = ...` alias (fully qualified); resolved to
                   // its target type by the middle/mono pass
    Tuple(Vec<Ty>), // `(T1, T2, ...)` — a multi-value return type
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
    /// (name, ty, span, default); the default (only on trailing params) is
    /// evaluated in the caller's context when the argument is omitted.
    pub params: Vec<(String, Option<Ty>, Span, Option<Expr>)>,
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
    // `for (T x : c)`: iterate the elements of `c` — an array (`len`/
    // `[]`) or a collection with `size()`/`get(i)` (std `List`,
    // `Queue`, `HashSet`, `CopyOnWriteList`). The monomorphizer
    // desugars it into an index loop; `ty` is `None` for `var x` (the
    // element type is inferred from the target).
    ForEach {
        span: Span,
        /// Loop label for `break name` / `continue name`.
        label: Option<String>,
        name: String,
        ty: Option<Ty>,
        target: Box<Expr>,
        body: Box<Block>,
        /// The target's resolved type, recorded by the monomorphizer's first
        /// pass so the post-expansion desugaring pass (which runs after
        /// generic class instances are expanded) can pick the access path.
        base_ty: Option<Ty>,
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
    // `defer expr;` - expr is evaluated when the enclosing function exits
    // (Go-style: last deferred runs first, runs on every return path).
    Defer {
        span: Span,
        expr: Box<Expr>,
    },
    // `try { ... } catch (T var) { ... }` / `catch (A | B var) { ... }` -
    // exception handling. `catch_types` has one entry per type in the list;
    // the catch variable itself is typed by their least upper bound (a
    // common parent class or interface).
    TryCatch {
        span: Span,
        try_block: Box<Block>,
        catch_types: Vec<Ty>,
        catch_var: String,
        catch_block: Box<Block>,
        /// Optional `finally { ... }` block; runs on normal completion, on a
        /// caught exception, and on a `return` from the try/catch blocks.
        finally: Option<Box<Block>>,
    },
    // `(T1 a, T2 b) = value;` — destructure a tuple-typed value into fresh
    // locals. `fields` is (type, name, name-span); each name-span is unique
    // so the backend can plan a slot per field.
    TupleDecl {
        span: Span,
        fields: Vec<(Ty, String, Span)>,
        value: Box<Expr>,
    },
    // `var (a, b) = value;` — destructure with inferred element types;
    // `tys` is filled in by the monomorphizer (empty in the parsed AST).
    TupleVar {
        span: Span,
        names: Vec<(String, Span)>,
        tys: Vec<Ty>,
        value: Box<Expr>,
    },
    // `(a, b) = value;` — destructure into existing locals.
    TupleAssign {
        span: Span,
        targets: Vec<Expr>,
        value: Box<Expr>,
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
    BitNot, // ~
    Deref,  // *
    Addr,   // &
    FnAddr, // @ (function address)
}

/// A pattern in a `match` arm.
#[derive(Debug, Clone)]
pub enum MatchPat {
    /// An integer (or char, which is a code point) literal.
    Int(i64),
    /// A string literal.
    Str(String),
    /// An enum variant (`Enum.Variant`); resolved to its int value by the
    /// monomorphizer.
    Enum {
        enum_name: String,
        variant: String,
    },
    /// The wildcard `_`, which matches anything and serves as the fallback.
    Wildcard,
}

/// One arm of a `match`: a pattern and the expression it yields.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub span: Span,
    pub pattern: MatchPat,
    pub then: Box<Expr>,
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
    // String interpolation `"a={x}b"`: parts alternate between literal text
    // fragments (`Expr::Str`) and interpolated expressions, starting and
    // ending with a fragment.
    Interp {
        span: Span,
        parts: Vec<Expr>,
    },
    Ident {
        span: Span,
        name: String,
    },
    This {
        span: Span,
    },
    // callee is a dotted path (e.g. ["sys", "write"] or ["println"]).
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
        /// Type arguments for a qualified generic free-function call
        /// (`std.map<T, U>(xs, f)`); empty for real method calls (which
        /// have no method-level generics in v1).
        type_args: Vec<Ty>,
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
    // A freshly allocated, zero-filled buffer of `size` elements:
    // `int[n]` (8-byte slots, length header in slot 0), `byte[n]` /
    // `char[n]` (n contiguous bytes), `short[n]` (n contiguous 2-byte
    // elements), or `string[n]` (a `Str` NUL-terminated buffer of `n`
    // bytes). The value owns its single reference; the owner frees it at
    // scope end.
    SizedNew {
        span: Span,
        size: Box<Expr>,
        ty: Ty,
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
    // `base?.rest`: if `base` is null the whole chain yields the zero value
    // of the result type, otherwise the chain is evaluated. `rest` is the
    // remainder of the postfix chain (field/method/index hops) whose root is
    // an `OptRef` naming the base.
    OptChain {
        span: Span,
        base: Box<Expr>,
        rest: Box<Expr>,
    },
    // The base of the enclosing optional chain (valid only inside an
    // `OptChain`'s `rest`).
    OptRef {
        span: Span,
    },
    // `l ?? r`: if `l` is null, `r`, else `l`.
    Coalesce {
        span: Span,
        l: Box<Expr>,
        r: Box<Expr>,
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
    // `(e1, e2, ...)` — a tuple literal (only meaningful as a `return`
    // value or the right-hand side of a tuple destructure).
    Tuple {
        span: Span,
        elems: Vec<Expr>,
    },
    // `match <scrutinee> { <pat> => <expr>, ... }` — pattern matching.
    // Supported scrutinee types: int, char (a code point), enum, and
    // string. The monomorphizer desugars it into a nested `Cond` chain.
    Match {
        span: Span,
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}
