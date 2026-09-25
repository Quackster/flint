use crate::ast::*;
use crate::error::CompileResult;
use std::collections::HashMap;

use super::Mono;

impl<'a> Mono<'a> {
    pub(crate) fn expand_struct(&mut self, ni: usize, ti: usize, args: &[Ty]) -> CompileResult<()> {
        let template = self.prog.structs[ti].clone();
        let tname = template.name.clone();
        let tpkg = template.package.clone();
        let subst = self.make_subst(&template.type_params, args);
        let name = self.class_names[&ni].clone();
        let mut c = template;
        c.name = name.clone();
        c.type_params = Vec::new(); // expanded copy is concrete
        // Mangle the parent/implemented names to match the mangled symbol names
        // (the class `name` is mangled; `extends`/`implements` must be too, or
        // the backend's name-based lookups (parent_idx, is_parent, is_subtype)
        // never match and inheritance/vtable dispatch silently breaks).
        c.extends = c.extends.as_ref().map(|p| super::mangle(&c.package, p));
        for i in c.implements.iter_mut() {
            // Qualified names (from the parser's interface resolution) carry
            // their own package; short names use the class's package.
            *i = super::mangle_fqn(&c.package, i);
        }
        for f in c.fields.iter_mut() {
            f.ty = self.resolve_type(&f.ty, &subst)?;
        }
        for m in c.methods.iter_mut() {
            // The constructor is named after the class; rename it to the
            // mangled class name so `mangle(name, ctor)` lines up in codegen.
            if m.is_ctor {
                m.name = name.clone();
            }
            for p in m.params.iter_mut() {
                if let Some(t) = &mut p.1 {
                    *t = self.resolve_type(t, &subst)?;
                }
            }
            if let Some(r) = &mut m.ret {
                *r = self.resolve_type(r, &subst)?;
            }
            let mut locals = HashMap::new();
            for p in &m.params {
                if let Some(t) = &p.1 {
                    locals.insert(p.0.clone(), t.clone());
                }
            }
            self.cur_func_package = c.package.clone();
            self.cur_locals = locals;
            m.body = self.resolve_block(&m.body, &subst)?;
        }
        // Bake the element `kind` (0 int / 1 string / 2 object) into the
        // stdlib collection constructors from the concrete type argument, so
        // `List<string>` hashes/compares by content and `List<Foo>`
        // refcounts its elements without a manual `kind` field set.
        if tpkg == "std" {
            let fields: Vec<(&str, i64)> = match (tname.as_str(), args) {
                ("List" | "Queue" | "HashSet", [a]) => vec![("kind", Self::kind_of(a))],
                ("HashMap", [a, b]) => vec![
                    ("kkind", Self::kind_of(a)),
                    ("vkind", Self::kind_of(b)),
                ],
                _ => Vec::new(),
            };
            if !fields.is_empty() {
                self.bake_kinds(&mut c, &fields);
            }
        }
        self.out.structs[ni] = c;
        Ok(())
    }

    /// The element flavour for a concrete type: 0 for integer-ish values
    /// (int/bool/enum/pointer/array), 1 for string, 2 for class objects.
    fn kind_of(t: &Ty) -> i64 {
        match t {
            Ty::Str => 1,
            Ty::Struct(_) | Ty::Inst(_, _) => 2,
            _ => 0,
        }
    }

    /// Rewrite `this.<field> = <int>` in the constructor to the baked value.
    fn bake_kinds(&self, c: &mut ClassDef, fields: &[(&str, i64)]) {
        let Some(ctor) = c.methods.iter_mut().find(|m| m.is_ctor) else {
            return;
        };
        for f in fields {
            let (fname, fval) = *f;
            self.bake_in_block(&mut ctor.body, fname, fval);
        }
    }

    fn bake_in_block(&self, block: &mut Block, fname: &str, fval: i64) {
        for s in block.stmts.iter_mut() {
            if let Stmt::Assign { target, value, .. } = s {
                if let Expr::Field { base, name, .. } = target {
                    if name == fname
                        && matches!(&**base, Expr::This { .. })
                        && matches!(value, Expr::Int { .. })
                    {
                        if let Expr::Int { span, .. } = value {
                            *value = Expr::Int { span: *span, value: fval };
                        }
                        continue;
                    }
                }
            }
            // Recurse into nested blocks so the assignment is found wherever it
            // lives in the constructor body.
            match s {
                Stmt::If { then, else_opt, .. } => {
                    self.bake_in_block(then, fname, fval);
                    if let Some(e) = else_opt {
                        self.bake_in_block(e, fname, fval);
                    }
                }
                Stmt::While { body, .. } => self.bake_in_block(body, fname, fval),
                Stmt::For { body, .. } => self.bake_in_block(body, fname, fval),
                Stmt::TryCatch {
                    try_block,
                    catch_block,
                    finally,
                    ..
                } => {
                    self.bake_in_block(try_block, fname, fval);
                    self.bake_in_block(catch_block, fname, fval);
                    if let Some(f) = finally {
                        self.bake_in_block(f, fname, fval);
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn expand_func(&mut self, ni: usize, ti: usize, args: &[Ty]) -> CompileResult<()> {
        let template = self.prog.funcs[ti].clone();
        let subst = self.make_subst(&template.type_params, args);
        let name = self.func_names[&ni].clone();
        let mut f = template;
        f.name = name;
        f.type_params = Vec::new(); // expanded copy is concrete
        let mut locals = HashMap::new();
        for p in f.params.iter_mut() {
            if let Some(t) = &mut p.1 {
                let rt = self.resolve_type(t, &subst)?;
                locals.insert(p.0.clone(), rt.clone());
                *t = rt;
            }
        }
        if let Some(r) = &mut f.ret {
            *r = self.resolve_type(r, &subst)?;
        }
        self.cur_func_package = f.package.clone();
        self.cur_locals = locals;
        f.body = self.resolve_block(&f.body, &subst)?;
        self.out.funcs[ni] = f;
        Ok(())
    }
}
