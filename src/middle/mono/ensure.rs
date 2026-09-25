use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use super::Mono;

impl<'a> Mono<'a> {
    /// Reserve (or fetch) the concrete struct for a template + type args.
    pub(crate) fn ensure_class(&mut self, idx: usize, args: Vec<Ty>) -> CompileResult<usize> {
        // Non-generic classes are already reserved in phase 1; reuse that slot.
        if self.prog.structs[idx].type_params.is_empty() {
            return Ok(self.struct_new[&idx]);
        }
        let key = (idx, args.clone());
        if let Some(&ni) = self.class_inst.get(&key) {
            return Ok(ni);
        }
        let (name, pkg, nparams) = {
            let t = &self.prog.structs[idx];
            (t.name.clone(), t.package.clone(), t.type_params.len())
        };
        if args.len() != nparams {
            return Err(CompileError::new(
                Span::new(0, 0),
                format!(
                    "class '{}' expects {} type argument(s), got {}",
                    name, nparams, args.len()
                ),
            ));
        }
        let ni = self.out.structs.len();
        self.out.structs.push(super::placeholder_class());
        let base = super::mangle(&pkg, &name);
        let mangled = if args.is_empty() {
            base
        } else {
            let frag: Vec<String> = args.iter().map(|a| self.mangle_ty(a)).collect();
            format!("{}_{}", base, frag.join("_"))
        };
        self.class_names.insert(ni, mangled);
        self.class_inst.insert(key, ni);
        self.class_work.push((idx, args));
        Ok(ni)
    }

    /// Reserve (or fetch) the concrete function for a template + type args.
    pub(crate) fn ensure_func(&mut self, idx: usize, args: Vec<Ty>) -> CompileResult<usize> {
        // Non-generic functions are already reserved in phase 1; reuse that slot.
        if self.prog.funcs[idx].type_params.is_empty() {
            return Ok(self.func_new[&idx]);
        }
        let key = (idx, args.clone());
        if let Some(&ni) = self.func_inst.get(&key) {
            return Ok(ni);
        }
        let (name, pkg, nparams) = {
            let t = &self.prog.funcs[idx];
            (t.name.clone(), t.package.clone(), t.type_params.len())
        };
        if args.len() != nparams {
            return Err(CompileError::new(
                Span::new(0, 0),
                format!(
                    "function '{}' expects {} type argument(s), got {}",
                    name, nparams, args.len()
                ),
            ));
        }
        let ni = self.out.funcs.len();
        self.out.funcs.push(super::placeholder_func());
        let base = super::mangle(&pkg, &name);
        let mangled = if args.is_empty() {
            base
        } else {
            let frag: Vec<String> = args.iter().map(|a| self.mangle_ty(a)).collect();
            format!("{}_{}", base, frag.join("_"))
        };
        self.func_names.insert(ni, mangled);
        self.func_inst.insert(key, ni);
        self.func_work.push((idx, args));
        Ok(ni)
    }

    /// Mangled name fragment for a (resolved) type argument.
    pub(crate) fn mangle_ty(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int => "int".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Ptr => "ptr".to_string(),
            Ty::Str => "string".to_string(),
            Ty::Array => "array".to_string(),
            Ty::Void => "void".to_string(),
            Ty::Struct(idx) => self
                .class_names
                .get(idx)
                .cloned()
                .unwrap_or_else(|| "struct".to_string()),
            Ty::Enum(idx) => self
                .prog
                .enums
                .get(*idx)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "enum".to_string()),
            Ty::Param(_) | Ty::Inst(_, _) => "type".to_string(),
            Ty::Interface(idx) => self
                .prog
                .interfaces
                .get(*idx)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| "interface".to_string()),
        }
    }
}
