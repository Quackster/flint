use crate::ast::*;
use crate::error::CompileResult;
use std::collections::HashMap;

use super::Mono;

impl<'a> Mono<'a> {
    pub(crate) fn expand_struct(&mut self, ni: usize, ti: usize, args: &[Ty]) -> CompileResult<()> {
        let template = self.prog.structs[ti].clone();
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
        self.out.structs[ni] = c;
        Ok(())
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
