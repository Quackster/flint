use crate::ast::{ClassDef, FieldDef, Ty};
use crate::error::{CompileError, CompileResult};

use super::{Ctx, getter_name, mangle, setter_name};

impl Ctx<'_> {
    pub(crate) fn gen_getter(&mut self, class_def: &ClassDef, field: &FieldDef) -> CompileResult<()> {
        let mangled = mangle(&class_def.name, &getter_name(&field.name));
        let sidx = *self
            .struct_idx
            .get(&class_def.name)
            .ok_or_else(|| CompileError::new(field.span, format!("unknown class '{}'", class_def.name)))?;
        let off = self.struct_field_offset(sidx, &field.name)?;
        let cname = &class_def.name;
        self.emit(&format!("{}:", mangled));
        self.emit(&format!(".type {}, @function", mangled));
        // getter: this in %rdi, return the field. The caller transferred a
        // reference on this, so release it before returning.
        match field.ty {
            Ty::Struct(fs) => {
                // class field: the returned value is a new reference (retain)
                self.emit(&format!("\tmovq {}(%rdi), %rax", off));
                self.emit("\tpush %rax");
                self.emit(&format!("\tcall flint_release_{}", cname));
                self.emit("\tmov (%rsp), %rdi");
                self.emit("\tcall flint_retain");
                self.emit("\tpop %rax");
            }
            _ => {
                self.emit(&format!("\tmovq {}(%rdi), %rax", off));
                self.emit("\tpush %rax");
                self.emit(&format!("\tcall flint_release_{}", cname));
                self.emit("\tpop %rax");
            }
        }
        self.emit("\tret");
        self.emit(&format!(".size {}, .-{}", mangled, mangled));
        Ok(())
    }

    pub(crate) fn gen_setter(&mut self, class_def: &ClassDef, field: &FieldDef) -> CompileResult<()> {
        let mangled = mangle(&class_def.name, &setter_name(&field.name));
        let sidx = *self
            .struct_idx
            .get(&class_def.name)
            .ok_or_else(|| CompileError::new(field.span, format!("unknown class '{}'", class_def.name)))?;
        let off = self.struct_field_offset(sidx, &field.name)?;
        let cname = &class_def.name;
        self.emit(&format!("{}:", mangled));
        self.emit(&format!(".type {}, @function", mangled));
        // setter: this in %rdi, value in %rsi (already retained by the
        // caller). Release this (transferred reference), release the old
        // field value, then store.
        self.emit("\tpush %rax");
        self.emit("\tpush %rsi");
        self.emit("\tpush %rdi");
        self.emit(&format!("\tmovq {}(%rsp), %r10", off)); // old field value
        self.emit(&format!("\tcall flint_release_{}", cname)); // release this
        self.emit("\tmov %r10, %rdi");
        self.emit("\ttest %rdi, %rdi");
        self.emit(&format!("\tjz .Lset_skip{}", self.strn));
        self.strn += 1;
        if let Ty::Struct(fs) = field.ty {
            let fname = &self.prog.structs[fs].name;
            self.emit(&format!("\tcall flint_release_{}", fname));
        }
        self.emit(&format!(".Lset_skip{}:", self.strn - 1));
        self.emit("\tpop %rdi");
        self.emit("\tpop %rsi");
        self.emit(&format!("\tmovq %rsi, {}(%rdi)", off));
        self.emit("\tpop %rax");
        self.emit("\txor %rax, %rax");
        self.emit("\tret");
        self.emit(&format!(".size {}, .-{}", mangled, mangled));
        Ok(())
    }
}
