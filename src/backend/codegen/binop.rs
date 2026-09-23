use crate::ast::{BinOp, Ty};
use crate::error::{CompileError, CompileResult};
use crate::span::Span;

use super::{Ctx, ty_name};

// v1: array slots are 64-bit values; `Ty::Array` compares and mixes with
// other 64-bit types exactly like `Ty::Int`.
fn as_cmp_ty(t: Ty) -> Ty {
    // Array slots and enum values are plain 64-bit ints, so they compare and
    // mix with `Ty::Int`.
    match t {
        Ty::Array | Ty::Enum(_) => Ty::Int,
        other => other,
    }
}

impl Ctx<'_> {
    pub(crate) fn check_binop_types(
        &self,
        op: BinOp,
        lt: Ty,
        rt: Ty,
        span: Span,
    ) -> CompileResult<()> {
        // logical && / || accept any 64-bit operand (truthiness)
        if matches!(op, BinOp::And | BinOp::Or) {
            return Ok(());
        }
        let cmp = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
        );
        let (lt, rt) = (as_cmp_ty(lt), as_cmp_ty(rt));
        let is_ptr = lt == Ty::Ptr || rt == Ty::Ptr || lt == Ty::Str || rt == Ty::Str;
        // all reference-ish types compare as pointers (e.g. `p == null`)
        let ptrish = |t: &Ty| {
            matches!(
                t,
                Ty::Ptr | Ty::Str | Ty::Array | Ty::Struct(_) | Ty::Interface(_) | Ty::List
                    | Ty::Queue | Ty::HashMap | Ty::HashSet
            )
        };
        if cmp {
            if lt != rt && !(ptrish(&lt) && ptrish(&rt)) {
                return Err(CompileError::new(
                    span,
                    format!(
                        "comparing mismatched types {} and {}",
                        ty_name(&lt),
                        ty_name(&rt)
                    ),
                ));
            }
        } else if is_ptr && (op == BinOp::Add || op == BinOp::Sub) {
            // pointer arithmetic allowed
        } else if lt != rt && lt != Ty::Int && rt != Ty::Int {
            return Err(CompileError::new(
                span,
                format!(
                    "binary operator on mismatched types {} and {}",
                    ty_name(&lt),
                    ty_name(&rt)
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn emit_binop(&mut self, op: BinOp) {
        match op {
            BinOp::Add => self.emit("\tadd %rsi, %rdi"),
            BinOp::Sub => self.emit("\tsub %rsi, %rdi"),
            BinOp::Mul => self.emit("\timul %rsi, %rdi"),
            // This toolchain's cltd only sign-extends the low 32 bits (cdq
            // behaviour) and idiv traps whenever rdx != 0, so cltd+idiv is
            // not usable for general signed division. Both ops go through
            // flint_sdiv/flint_smod (unsigned div on absolute values, rdx
            // explicitly zeroed); rdi = dividend, rsi = divisor, rax = result.
            BinOp::Div => {
                self.emit("\tcall flint_sdiv");
                self.emit("\tmov %rax, %rdi");
            }
            BinOp::Mod => {
                self.emit("\tcall flint_smod");
                self.emit("\tmov %rax, %rdi");
            }
            // `&&` / `||` never reach here; they are emitted with short
            // circuiting in gen_expr.
            BinOp::And => self.emit("\tand %rsi, %rdi"),
            BinOp::Or => self.emit("\tor %rsi, %rdi"),
            BinOp::BitAnd => self.emit("\tand %rsi, %rdi"),
            BinOp::BitOr => self.emit("\tor %rsi, %rdi"),
            BinOp::BitXor => self.emit("\txor %rsi, %rdi"),
            BinOp::Shl => {
                self.emit("\tmov %rsi, %rcx");
                self.emit("\tshl %cl, %rdi");
            }
            BinOp::Shr => {
                self.emit("\tmov %rsi, %rcx");
                self.emit("\tsar %cl, %rdi");
            }
            BinOp::Eq => {
                self.emit("\tcmp %rsi, %rdi");
                self.emit("\tsete %al");
                self.emit("\tmovzbl %al, %edi");
            }
            BinOp::Ne => {
                self.emit("\tcmp %rsi, %rdi");
                self.emit("\tsetne %al");
                self.emit("\tmovzbl %al, %edi");
            }
            BinOp::Lt => {
                self.emit("\tcmp %rsi, %rdi");
                self.emit("\tsetl %al");
                self.emit("\tmovzbl %al, %edi");
            }
            BinOp::Gt => {
                self.emit("\tcmp %rsi, %rdi");
                self.emit("\tsetg %al");
                self.emit("\tmovzbl %al, %edi");
            }
            BinOp::Le => {
                self.emit("\tcmp %rsi, %rdi");
                self.emit("\tsetle %al");
                self.emit("\tmovzbl %al, %edi");
            }
            BinOp::Ge => {
                self.emit("\tcmp %rsi, %rdi");
                self.emit("\tsetge %al");
                self.emit("\tmovzbl %al, %edi");
            }
        }
    }
}
