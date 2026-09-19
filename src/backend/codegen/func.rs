use crate::ast::{Block, ClassDef, CollKind, FuncDef, MethodDef, Stmt, Ty};
use crate::error::{CompileError, CompileResult};

use crate::backend::escape::{self, LocalKind};
use crate::backend::layout;
use super::{ARGREGS, Ctx, Frame, Local, mangle, struct_idx_of};

impl Ctx<'_> {
    pub(crate) fn gen_func(&mut self, f: &FuncDef) -> CompileResult<()> {
        if f.params.len() > 6 {
            return Err(CompileError::new(
                f.span,
                "functions may have at most 6 parameters",
            ));
        }
        let plan = escape::plan_func(self.prog, &f.params, &f.body, false, None);
        let k = plan.frame_bytes;

        self.emit(&format!("{}:", f.name));
        self.emit(&format!(".type {}, @function", f.name));
        self.emit("\tpush %rbp");
        self.emit("\tmov %rsp, %rbp");
        let sub_pos = self.out.len();
        if k > 0 {
            self.emit(&format!("\tsub ${}, %rsp", k));
        }
        let nparams = f.params.len();
        let mut frame = Frame::new(nparams);
        frame.ret_type = f.ret.clone();
        // store params into frame
        for (i, (pname, pty, _)) in f.params.iter().enumerate() {
            let reg = ARGREGS[i];
            let off = frame.param_slot(i);
            self.emit(&format!("\tmov {}, {}(%rbp)", reg, off));
            frame.locals.push(Local {
                name: pname.clone(),
                off,
                ty: pty.clone().unwrap_or(Ty::Int),
                kind: plan.param_kinds[i],
                region: None,
            });
        }

        self.prewalk_decls(&f.body, &mut frame, &plan);
        self.zero_class_slots(&frame, nparams);
        self.gen_block(&f.body, &mut frame)?;

        // implicit end: release heap refs, then fall through returns 0
        self.emit_end_releases(&frame);
        self.emit("\txor %rax, %rax");
        self.emit("\tleave");
        self.emit("\tret");
        self.reserve_frame(&frame, sub_pos);
        self.emit(&format!(".size {}, .-{}", f.name, f.name));
        Ok(())
    }

    /// Widen the prologue's `sub` so it covers every slot actually allocated
    /// (including dynamic ones: catch vars, finally val/flag slots), and emit
    /// the matching `add` before each `leave` in this function. Without the
    /// reservation, a `call`'s return-address push and the callee's frame
    /// overwrite the caller's slots.
    fn reserve_frame(&mut self, frame: &Frame, sub_pos: usize) {
        let need = (frame.next_bytes + 15) / 16 * 16;
        if need == 0 {
            return;
        }
        let has_sub = self
            .out
            .get(sub_pos)
            .map(|l| l.starts_with("\tsub "))
            .unwrap_or(false);
        let line = format!("\tsub ${}, %rsp", need);
        if has_sub {
            self.out[sub_pos] = line;
        } else {
            self.out.insert(sub_pos, line);
        }
        let end = self.out.len();
        for i in (sub_pos + 1..end).rev() {
            if self.out[i] == "\tleave" {
                self.out.insert(i, format!("\tadd ${}, %rsp", need));
            }
        }
    }

    pub(crate) fn gen_method(&mut self, class_def: &ClassDef, meth: &MethodDef) -> CompileResult<()> {
        let mangled = mangle(&class_def.name, &meth.name);
        let sidx = *self.struct_idx.get(&class_def.name).unwrap_or(&0);
        let plan =
            escape::plan_func(self.prog, &meth.params, &meth.body, !meth.is_static, Some(sidx));
        let nparams = meth.params.len() + if meth.is_static { 0 } else { 1 };
        if nparams > 6 {
            return Err(CompileError::new(
                meth.span,
                format!("method '{}' has too many parameters (including this)", meth.name),
            ));
        }
        let mut frame = Frame::new(nparams);
        frame.ret_type = meth.ret.clone();
        let k = plan.frame_bytes;
        self.emit(&format!("{}:", mangled));
        self.emit(&format!(".type {}, @function", mangled));
        self.emit("\tpush %rbp");
        self.emit("\tmov %rsp, %rbp");
        let sub_pos = self.out.len();
        if k > 0 {
            self.emit(&format!("\tsub ${}, %rsp", k));
        }
        if !meth.is_static {
            frame.this_class = Some(sidx);
            // this is slot 0
            frame.this_offset = Some(frame.param_slot(0));
            // `this` is borrowed, not owned: the caller retains the reference
            // and releases it. The method must not release `this`.
            frame.release_this = false;
        }
        // store params: this at slot 0, then explicit params
        if !meth.is_static {
            let off = frame.param_slot(0);
            self.emit(&format!("\tmov {}, {}(%rbp)", ARGREGS[0], off));
            // type of this is *Class (pointer to struct)
            frame.locals.push(Local {
                name: "this".to_string(),
                off,
                ty: Ty::Struct(sidx),
                kind: plan.param_kinds[0],
                region: None,
            });
        }
        for (i, (pname, pty, _)) in meth.params.iter().enumerate() {
            let slot = if meth.is_static { i } else { i + 1 };
            let reg = ARGREGS[slot];
            let off = frame.param_slot(slot);
            self.emit(&format!("\tmov {}, {}(%rbp)", reg, off));
            frame.locals.push(Local {
                name: pname.clone(),
                off,
                ty: pty.clone().unwrap_or(Ty::Int),
                kind: plan.param_kinds[slot],
                region: None,
            });
        }
        self.prewalk_decls(&meth.body, &mut frame, &plan);
        self.zero_class_slots(&frame, nparams);
        self.gen_block(&meth.body, &mut frame)?;
        // release heap refs (params, this, locals) before returning
        self.emit_end_releases(&frame);
        self.emit("\txor %rax, %rax");
        self.emit("\tleave");
        self.emit("\tret");
        self.reserve_frame(&frame, sub_pos);
        self.emit(&format!(".size {}, .-{}", mangled, mangled));
        Ok(())
    }

    /// Pre-allocate a slot (and stack-object region) for every decl, in the
    /// same source order as the escape pre-pass.
    pub(crate) fn prewalk_decls(&mut self, block: &Block, frame: &mut Frame, plan: &escape::FuncPlan) {
        escape::for_each_decl(block, |stmt| {
            if let Stmt::Decl { span, name, .. } = stmt {
                if let Some(dp) = plan.decls.get(span) {
                    let off = frame.slot();
                    let region = match dp.nslots {
                        Some(ns) => Some(frame.region(ns * 8)),
                        None => None,
                    };
                    frame.locals.push(Local {
                        name: name.clone(),
                        off,
                        ty: dp.ty.clone().unwrap_or(Ty::Int),
                        kind: dp.kind,
                        region,
                    });
                }
            }
        });
    }

    /// Zero heap-class slots (so the first decl's old-release sees a null)
    /// and zero stack-object regions (header + fields) so the first `new`
    /// into a region sees null class fields.
    pub(crate) fn zero_class_slots(&mut self, frame: &Frame, nparams: usize) {
        for l in frame.locals.iter().skip(nparams) {
            match l.kind {
                LocalKind::Heap => {
                    self.emit(&format!("\tmovq $0, {}(%rbp)", l.off));
                }
                LocalKind::StackOwner => {
                    if let (Some(roff), Some(s)) = (l.region, struct_idx_of(&l.ty)) {
                        for i in 0..layout::nslots(self.prog, s) {
                            self.emit(&format!(
                                "\tmovq $0, {}(%rbp)",
                                roff + i as i64 * 8
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Release every heap reference held by the frame (params, this, locals)
    /// and the class-typed fields of stack-allocated objects.
    pub(crate) fn emit_end_releases(&mut self, frame: &Frame) {
        let skip_this = frame.this_class.is_some() && !frame.release_this;
        for (i, l) in frame.locals.iter().enumerate() {
            if skip_this && i == 0 {
                continue;
            }
            match l.kind {
                LocalKind::Heap => {
                    if let Some(s) = struct_idx_of(&l.ty) {
                        let cname = &self.prog.structs[s].name;
                        self.emit(&format!("\tmovq {}(%rbp), %rdi", l.off));
                        self.emit(&format!("\tcall flint_release_{}", cname));
                    } else if let Some(k) = l.ty.coll_kind() {
                        let rname = match k {
                            CollKind::List => "flint_release_list",
                            CollKind::Queue => "flint_release_queue",
                            CollKind::Set => "flint_release_set",
                            CollKind::Map => "flint_release_map",
                        };
                        self.emit(&format!("\tmovq {}(%rbp), %rdi", l.off));
                        self.emit(&format!("\tcall {}", rname));
                    }
                }
                LocalKind::StackOwner => {
                    if let Some(s) = struct_idx_of(&l.ty) {
                        if let Some(roff) = l.region {
                            let base = layout::field_base_offset(self.prog, s);
                            for (fi, (_, fty)) in layout::all_fields(self.prog, s).iter().enumerate() {
                                if let Some(fs) = struct_idx_of(fty) {
                                    let fname = &self.prog.structs[fs].name;
                                    self.emit(&format!(
                                        "\tmovq {}(%rbp), %rdi",
                                        roff + base + fi as i64 * 8
                                    ));
                                    self.emit(&format!("\tcall flint_release_{}", fname));
                                } else if let Some(k) = fty.coll_kind() {
                                    let rname = match k {
                                        CollKind::List => "flint_release_list",
                                        CollKind::Queue => "flint_release_queue",
                                        CollKind::Set => "flint_release_set",
                                        CollKind::Map => "flint_release_map",
                                    };
                                    self.emit(&format!(
                                        "\tmovq {}(%rbp), %rdi",
                                        roff + base + fi as i64 * 8
                                    ));
                                    self.emit(&format!("\tcall {}", rname));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
