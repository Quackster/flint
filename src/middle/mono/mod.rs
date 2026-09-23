use crate::ast::*;
use crate::error::CompileResult;
use std::collections::HashMap;

mod ensure;
mod expand;
mod infer;
mod resolve;
mod resolve_expr;

/// Fully qualified name with dots: `package.Name`, or just `Name` when the
/// package is empty. Used as the key in the name maps.
pub(crate) fn fqn(package: &str, name: &str) -> String {
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", package, name)
    }
}

/// Mangled (symbol) name: `package_Name` with dots -> underscores, or just
/// `Name` when the package is empty. `main` is always the bare entry symbol.
pub(crate) fn mangle(package: &str, name: &str) -> String {
    if name == "main" {
        return "main".to_string();
    }
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{}_{}", package.replace('.', "_"), name)
    }
}

/// Mangle a possibly-qualified name (`a.b.Name` or `Name`). A qualified name
/// carries its own package and is mangled with it; a short name is mangled
/// with the caller's (e.g. the class's) package.
pub(crate) fn mangle_fqn(pkg: &str, name: &str) -> String {
    match name.rfind('.') {
        Some(pos) => mangle(&name[..pos], &name[pos + 1..]),
        None => mangle(pkg, name),
    }
}

/// Expand generic classes and functions into concrete monomorphs.
///
/// The parser emits `Ty::Param` / `Ty::Inst` and `type_args` for generic
/// declarations and use sites. This pass walks the program, instantiates
/// every generic type/function that is actually used (via a worklist, so
/// nested instantiations like `Vessel<Pair<int, string>>` are handled), and
/// rewrites the AST so the backend only ever sees concrete `Ty::Struct` /
/// `Ty::Coll` types and mangled names.
pub fn monomorphize(prog: &Program) -> CompileResult<Program> {
    let mut m = Mono {
        prog,
        out: Program {
            structs: Vec::new(),
            funcs: Vec::new(),
            interfaces: Vec::new(),
            enums: Vec::new(),
            imports: Vec::new(),
        },
        struct_new: HashMap::new(),
        func_new: HashMap::new(),
        class_by_name: HashMap::new(),
        func_by_name: HashMap::new(),
        class_names: HashMap::new(),
        func_names: HashMap::new(),
        class_inst: HashMap::new(),
        func_inst: HashMap::new(),
        class_work: Vec::new(),
        func_work: Vec::new(),
        cur_locals: HashMap::new(),
        cur_func_package: String::new(),
    };
    m.run()
}

struct Mono<'a> {
    prog: &'a Program,
    out: Program,
    /// Original non-generic struct index -> new index.
    struct_new: HashMap<usize, usize>,
    /// Original non-generic function index -> new index.
    func_new: HashMap<usize, usize>,
    class_by_name: HashMap<String, usize>,
    func_by_name: HashMap<String, usize>,
    /// New struct index -> (mangled) name.
    class_names: HashMap<usize, String>,
    /// New function index -> (mangled) name.
    func_names: HashMap<usize, String>,
    /// (template index, type args) -> new struct index.
    class_inst: HashMap<(usize, Vec<Ty>), usize>,
    /// (template index, type args) -> new function index.
    func_inst: HashMap<(usize, Vec<Ty>), usize>,
    class_work: Vec<(usize, Vec<Ty>)>,
    func_work: Vec<(usize, Vec<Ty>)>,
    /// Local variable types for the body currently being resolved (inference).
    cur_locals: HashMap<String, Ty>,
    /// Package of the function/method body currently being resolved, used to
    /// resolve short (unqualified) free-function calls.
    cur_func_package: String,
}

impl<'a> Mono<'a> {
    fn run(&mut self) -> CompileResult<Program> {
        for (i, s) in self.prog.structs.iter().enumerate() {
            self.class_by_name.insert(fqn(&s.package, &s.name), i);
        }
        for (i, f) in self.prog.funcs.iter().enumerate() {
            self.func_by_name.insert(fqn(&f.package, &f.name), i);
        }
        // Interfaces are non-generic in v1; pass them through with mangled
        // names (so the backend's name-based lookups match the mangled
        // `implements` entries).
        for i in self.prog.interfaces.iter() {
            let mut c = i.clone();
            c.name = mangle(&i.package, &i.name);
            self.out.interfaces.push(c);
        }
        // Enums are non-generic in v1; pass them through unchanged.
        self.out.enums = self.prog.enums.clone();
        // Reserve slots for every non-generic struct and function (stable order).
        for (i, s) in self.prog.structs.iter().enumerate() {
            if s.type_params.is_empty() {
                let ni = self.out.structs.len();
                self.out.structs.push(placeholder_class());
                self.struct_new.insert(i, ni);
                self.class_names.insert(ni, mangle(&s.package, &s.name));
            }
        }
        for (i, f) in self.prog.funcs.iter().enumerate() {
            if f.type_params.is_empty() {
                let ni = self.out.funcs.len();
                self.out.funcs.push(placeholder_func());
                self.func_new.insert(i, ni);
                self.func_names.insert(ni, mangle(&f.package, &f.name));
            }
        }
        // Expand non-generic structs and functions (empty substitution).
        for i in 0..self.prog.structs.len() {
            if self.prog.structs[i].type_params.is_empty() {
                let ni = self.struct_new[&i];
                self.expand_struct(ni, i, &[])?;
            }
        }
        for i in 0..self.prog.funcs.len() {
            if self.prog.funcs[i].type_params.is_empty() {
                let ni = self.func_new[&i];
                self.expand_func(ni, i, &[])?;
            }
        }
        // Drain the instantiation worklists to a fixpoint.
        while let Some((ti, args)) = self.class_work.pop() {
            let ni = self.class_inst[&(ti, args.clone())];
            self.expand_struct(ni, ti, &args)?;
        }
        while let Some((ti, args)) = self.func_work.pop() {
            let ni = self.func_inst[&(ti, args.clone())];
            self.expand_func(ni, ti, &args)?;
        }
        Ok(std::mem::replace(
            &mut self.out,
            Program {
                structs: Vec::new(),
                funcs: Vec::new(),
                interfaces: Vec::new(),
                enums: Vec::new(),
                imports: Vec::new(),
            },
        ))
    }

    fn make_subst(&self, type_params: &[String], args: &[Ty]) -> Vec<(String, Ty)> {
        type_params
            .iter()
            .zip(args.iter())
            .map(|(p, a)| (p.clone(), a.clone()))
            .collect()
    }

    /// Resolve a free-function call path to a function index. A qualified path
    /// (e.g. `com.example.foo`) is looked up directly; a single short name is
    /// resolved against the current function's package.
    fn resolve_func(&self, callee: &[String]) -> Option<usize> {
        // A short name resolves against the current function's package first
        // (matching the parser's class-name precedence), then the default
        // package (or the qualified path, for multi-segment callees).
        if callee.len() == 1 {
            let fqn = fqn(&self.cur_func_package, &callee[0]);
            if let Some(&i) = self.func_by_name.get(&fqn) {
                return Some(i);
            }
        }
        let joined = callee.join(".");
        if let Some(&i) = self.func_by_name.get(&joined) {
            return Some(i);
        }
        if callee.len() == 1 {
            // Specific import: `import a.b.f` makes `f` available.
            for imp in &self.prog.imports {
                if !imp.ends_with(".*") && imp.rsplit('.').next() == Some(callee[0].as_str()) {
                    if let Some(&i) = self.func_by_name.get(imp.as_str()) {
                        return Some(i);
                    }
                }
            }
            // Wildcard import: `import a.b.*` makes `a.b.f` available.
            for imp in &self.prog.imports {
                if let Some(prefix) = imp.strip_suffix(".*") {
                    let f = format!("{}.{}", prefix, callee[0]);
                    if let Some(&i) = self.func_by_name.get(&f) {
                        return Some(i);
                    }
                }
            }
        }
        None
    }
}

fn placeholder_class() -> ClassDef {
    ClassDef {
        span: crate::span::Span::new(0, 0),
        package: String::new(),
        name: String::from("__placeholder__"),
        type_params: Vec::new(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        fields: Vec::new(),
        methods: Vec::new(),
    }
}

fn placeholder_func() -> FuncDef {
    FuncDef {
        span: crate::span::Span::new(0, 0),
        package: String::new(),
        name: String::from("__placeholder__"),
        type_params: Vec::new(),
        params: Vec::new(),
        ret: None,
        body: Block {
            span: crate::span::Span::new(0, 0),
            stmts: Vec::new(),
        },
    }
}
