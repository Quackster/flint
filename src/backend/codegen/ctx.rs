use crate::ast::{Program, Ty};
use std::collections::HashMap;

use crate::backend::escape::LocalKind;
use crate::error::CompileResult;

pub(crate) struct Ctx<'a> {
    pub prog: &'a Program,
    pub out: Vec<String>,
    pub rodata: Vec<String>,
    pub strn: usize,
    pub func_idx: HashMap<String, usize>,
    pub struct_idx: HashMap<String, usize>,
    // For methods: class_name -> method_name -> (class_idx, method_idx)
    pub method_map: HashMap<String, HashMap<String, (usize, usize)>>,
    /// Pending stack-object region for the next `new`: (region offset, nslots).
    pub stack_region: Option<(i64, usize)>,
    /// The type the receiver of the value currently being generated was
    /// declared with (set for `Ty name = e;`). Collection `get`/`pop`/`peek`
    /// and `list[i]` use it to pick the int or string result flavour.
    pub coll_expect: Option<Ty>,
    /// True while the value being generated is STORED into a `string`-typed
    /// local/field/param/return slot: `Expr::Str` then lowers to a fresh
    /// writable `flint_strcopy` (the "retain": each store owns an
    /// independent block; release is the v1 no-op — blocks reclaim at exit).
    pub str_copy: bool,
}

pub(crate) struct Local {
    pub name: String,
    /// Byte offset from %rbp.
    pub off: i64,
    pub ty: Ty,
    pub kind: LocalKind,
    /// Stack-object region offset for LocalKind::StackOwner.
    pub region: Option<i64>,
}

pub(crate) struct Frame {
    /// Locals in allocation order.
    pub locals: Vec<Local>,
    /// Bytes reserved below %rbp so far (params count).
    pub(crate) next_bytes: usize,
    pub this_class: Option<usize>,
    pub this_offset: Option<i64>,
    /// False for ctors: the caller keeps ownership of the `this` reference
    /// (the constructed object is the caller's, not the callee's).
    pub release_this: bool,
    /// Innermost enclosing loops/switches: (label, continue target, break
    /// target). `label` is set for labeled loops; `continue` is None for a
    /// switch (breakable, but not continuable).
    pub jump_stack: Vec<(Option<String>, Option<String>, String)>,
    /// Return type of the enclosing function/method (flavours collection
    /// reads in `return e;`).
    pub ret_type: Option<Ty>,
    /// Stack of try-block end labels (for `throw` to jump to).
    pub try_end_stack: Vec<String>,
    /// Active `finally` return capture: (jump target after the finally
    /// block, value slot, flag slot). When set, `return` stores its value,
    /// sets the flag, and jumps to the target instead of returning.
    pub ret_capture: Option<(String, i64, i64)>,
}

impl Frame {
    pub(crate) fn new(nparams: usize) -> Self {
        Self {
            locals: Vec::new(),
            next_bytes: nparams * 8,
            this_class: None,
            this_offset: None,
            release_this: true,
            jump_stack: Vec::new(),
            ret_type: None,
            try_end_stack: Vec::new(),
            ret_capture: None,
        }
    }
    pub(crate) fn param_slot(&self, i: usize) -> i64 {
        -((i as i64) + 1) * 8
    }
    /// Reserve an 8-byte local slot; returns its offset from %rbp.
    pub(crate) fn slot(&mut self) -> i64 {
        let off = -((self.next_bytes + 8) as i64);
        self.next_bytes += 8;
        off
    }
    /// Reserve a 16-aligned stack-object region; returns its offset from %rbp.
    pub(crate) fn region(&mut self, bytes: usize) -> i64 {
        let pad = (16 - self.next_bytes % 16) % 16;
        self.next_bytes += pad;
        let off = -((self.next_bytes + bytes) as i64);
        self.next_bytes += bytes;
        off
    }
    pub(crate) fn find(&self, name: &str) -> Option<&Local> {
        // innermost (last) binding wins
        self.locals.iter().rev().find(|l| l.name == name)
    }
}

impl Ctx<'_> {
    pub(crate) fn emit(&mut self, line: &str) {
        self.out.push(line.to_string());
    }

    /// Generate `e` in a read-only context (an operand that is consumed but
    /// never stored): a string literal stays on the read-only pool instead
    /// of being copied.
    pub(crate) fn gen_expr_ro(
        &mut self,
        e: &crate::ast::Expr,
        frame: &mut Frame,
    ) -> CompileResult<Ty> {
        let saved = self.str_copy;
        self.str_copy = false;
        let r = self.gen_expr(e, frame);
        self.str_copy = saved;
        r
    }

    /// Look up a function by its original name and return its mangled name.
    pub(crate) fn lookup_func_name(&self, name: &str) -> String {
        // Try the exact name first (might already be mangled)
        if let Some(&idx) = self.func_idx.get(name) {
            return self.prog.funcs[idx].name.clone();
        }
        // Try to find a function whose mangled name ends with the original name
        for (i, f) in self.prog.funcs.iter().enumerate() {
            if f.name.ends_with(name) {
                return f.name.clone();
            }
        }
        // Fall back to the original name
        name.to_string()
    }

    pub(crate) fn new_label(&mut self, s: &str) -> String {
        let label = format!(".Lstr{}", self.strn);
        self.strn += 1;
        // escape for .ascii: backslash and quote
        let mut esc = String::new();
        for c in s.chars() {
            match c {
                '\\' => esc.push_str("\\\\"),
                '"' => esc.push_str("\\\""),
                '\n' => esc.push_str("\\n"),
                '\t' => esc.push_str("\\t"),
                '\r' => esc.push_str("\\r"),
                '\0' => esc.push_str("\\0"),
                other => esc.push(other),
            }
        }
        self.rodata
            .push(format!("{}:\t.asciz \"{}\"", label, esc));
        label
    }
}
