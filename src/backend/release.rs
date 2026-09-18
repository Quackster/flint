// Per-class release functions. A class value is a pointer to a heap object
// laid out as [refcount:8][field0:8][field1:8]... (see flint_alloc in
// intrinsics.s). flint_release_<C> decrements the refcount and, when it reaches
// zero, recursively releases class-typed fields and munmaps the object.
// Cyclic structures therefore leak (each cycle edge holds a reference that
// keeps the refcount above zero) but never crash.
use super::codegen::{Ctx, coll_release_name, struct_idx_of};
use super::layout;

impl Ctx<'_> {
    /// Emit `flint_release_<C>` for one class.
    pub fn emit_release_fn(&mut self, sidx: usize) {
        let s = &self.prog.structs[sidx];
        let name = format!("flint_release_{}", s.name);
        // Must match flint_alloc's rounding: align16(nslots*8).
        let size = (layout::nslots(self.prog, sidx) as i64 * 8 + 15) / 16 * 16;
        self.emit(&format!(".globl {}", name));
        self.emit(&format!(".type {}, @function", name));
        self.emit(&format!("{}:", name));
        self.emit("\tpush %rbx");
        self.emit("\ttest %rdi, %rdi");
        self.emit(&format!("\tjz .Lrc_{}_done", s.name));
        self.emit("\tmovq (%rdi), %r11");
        self.emit("\ttest %r11, %r11");
        self.emit(&format!("\tjz .Lrc_{}_done", s.name));
        self.emit("\tdecq (%rdi)");
        self.emit(&format!("\tjnz .Lrc_{}_done", s.name));
        self.emit("\tmov %rdi, %rbx");
        let base = layout::field_base_offset(self.prog, sidx);
        for (i, (_, fty)) in layout::all_fields(self.prog, sidx).iter().enumerate() {
            let off = base + i as i64 * 8;
            if let Some(fs) = struct_idx_of(fty) {
                // class-typed field: release recursively
                let fname = &self.prog.structs[fs].name;
                self.emit(&format!("\tmovq {}(%rbx), %rdi", off));
                self.emit("\ttest %rdi, %rdi");
                self.emit(&format!("\tjz .Lrc_{}_f{}", s.name, i));
                self.emit(&format!("\tcall flint_release_{}", fname));
                self.emit(&format!(".Lrc_{}_f{}:", s.name, i));
            } else if let Some(k) = fty.coll_kind() {
                // collection-typed field: release (frees the collection and its
                // object elements)
                self.emit(&format!("\tmovq {}(%rbx), %rdi", off));
                self.emit("\ttest %rdi, %rdi");
                self.emit(&format!("\tjz .Lrc_{}_f{}", s.name, i));
                self.emit(&format!("\tcall {}", coll_release_name(k)));
                self.emit(&format!(".Lrc_{}_f{}:", s.name, i));
            }
        }
        self.emit("\tmov %rbx, %rdi");
        self.emit(&format!("\tmov ${}, %rsi", size));
        self.emit("\tmov $11, %rax"); // SYS_munmap
        self.emit("\tsyscall");
        self.emit(&format!(".Lrc_{}_done:", s.name));
        self.emit("\tpop %rbx");
        self.emit("\tret");
        self.emit(&format!(".size {}, .-{}", name, name));
    }
}
