use super::{Ctx, struct_idx_of};
use crate::backend::layout;

impl Ctx<'_> {
    /// Emit code leaving a fresh object base address in %rax. Either heap
    /// (flint_alloc + refcount header = 1) or the pending stack region, whose
    /// old class fields are released (no-op the first time: regions are
    /// zeroed at function entry) and whose slots are re-zeroed. A class with
    /// a vtable slot also gets its vtable pointer (slot 1) set.
    pub(crate) fn emit_new_base(&mut self, sidx: usize, region: Option<(i64, usize)>) {
        let has_vt = layout::has_vtable_slot(self.prog, sidx);
        let base = layout::field_base_offset(self.prog, sidx);
        let ns = layout::nslots(self.prog, sidx);
        match region {
            Some((roff, _ns)) => {
                // Hold the region base in %rdx: flint_release clobbers %rax (the
                // munmap syscall return), so the zeroing/vtable steps must not
                // depend on %rax surviving the release calls.
                self.emit(&format!("\tlea {}(%rbp), %rdx", roff));
                // release old class fields (at their field offsets)
                for (i, (_, fty)) in layout::all_fields(self.prog, sidx).iter().enumerate() {
                    if let Some(fs) = struct_idx_of(fty) {
                        let fname = &self.prog.structs[fs].name;
                        let skip = format!(".Lnb_skip{}", self.strn);
                        self.strn += 1;
                        self.emit(&format!("\tmovq {}(%rdx), %rdi", base + i as i64 * 8));
                        self.emit("\ttest %rdi, %rdi");
                        self.emit(&format!("\tjz {}", skip));
                        self.emit(&format!("\tcall flint_release_{}", fname));
                        self.emit(&format!("{}:", skip));
                    }
                }
                // zero all slots (header + fields)
                for i in 0..ns {
                    self.emit(&format!("\tmovq $0, {}(%rdx)", i * 8));
                }
                // set the vtable pointer (slot 1) if this class has one
                if has_vt {
                    let vt = layout::vtable_symbol(self.prog, sidx);
                    self.emit(&format!("\tlea {}(%rip), %rdi", vt));
                    self.emit("\tmovq %rdi, 8(%rdx)");
                }
                self.emit("\tmov %rdx, %rax"); // restore the base in %rax
            }
            None => {
                self.emit(&format!("\tmovq $({} * 8), %rdi", ns));
                self.emit("\tcall flint_alloc");
                self.emit("\tmovq $1, (%rax)"); // refcount at slot 0
                // set the vtable pointer (slot 1) if this class has one
                if has_vt {
                    let vt = layout::vtable_symbol(self.prog, sidx);
                    self.emit(&format!("\tlea {}(%rip), %rdi", vt));
                    self.emit("\tmovq %rdi, 8(%rax)");
                }
            }
        }
    }
}
