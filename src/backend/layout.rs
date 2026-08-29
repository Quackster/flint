// Inheritance and object-layout helpers. Pure functions over &Program so both
// escape analysis and codegen can share them.
//
// Object layout (8-byte slots):
//   no vtable slot:   [refcount][field0][field1]...
//   with vtable slot: [refcount][vtable][parent fields...][own fields...]
//
// The refcount is always at slot 0, so the generic flint_retain/flint_release and
// the per-class flint_release_<C> work unchanged. A class gets a vtable slot
// (and a vtable) when it extends a parent, implements an interface, or is a
// parent of another class.
use crate::ast::{Program, Ty};

/// The parent class index of `sidx`, if any.
pub(crate) fn parent_idx(prog: &Program, sidx: usize) -> Option<usize> {
    let name = prog.structs[sidx].extends.as_deref()?;
    prog.structs.iter().position(|s| s.name == name)
}

/// True when `sidx` is a parent of at least one other class.
pub(crate) fn is_parent(prog: &Program, sidx: usize) -> bool {
    let name = &prog.structs[sidx].name;
    prog.structs.iter().any(|s| s.extends.as_deref() == Some(name.as_str()))
}

/// True when the class needs a vtable slot (and hence a vtable).
pub(crate) fn has_vtable_slot(prog: &Program, sidx: usize) -> bool {
    let s = &prog.structs[sidx];
    s.extends.is_some() || !s.implements.is_empty() || is_parent(prog, sidx)
}

/// All non-static fields of `sidx` (parent's fields first, then own), as (name, type).
pub(crate) fn all_fields(prog: &Program, sidx: usize) -> Vec<(String, Ty)> {
    let mut out = Vec::new();
    if let Some(p) = parent_idx(prog, sidx) {
        out.extend(all_fields(prog, p));
    }
    for f in &prog.structs[sidx].fields {
        if !f.is_static {
            out.push((f.name.clone(), f.ty.clone()));
        }
    }
    out
}

/// All static fields of `sidx` (parent's fields first, then own), as (name, type).
pub(crate) fn all_static_fields(prog: &Program, sidx: usize) -> Vec<(String, Ty)> {
    let mut out = Vec::new();
    if let Some(p) = parent_idx(prog, sidx) {
        out.extend(all_static_fields(prog, p));
    }
    for f in &prog.structs[sidx].fields {
        if f.is_static {
            out.push((f.name.clone(), f.ty.clone()));
        }
    }
    out
}

/// Total field count (parent + own, non-static only).
pub(crate) fn total_fields(prog: &Program, sidx: usize) -> usize {
    all_fields(prog, sidx).len()
}

/// Number of 8-byte slots in the object (header + fields).
pub(crate) fn nslots(prog: &Program, sidx: usize) -> usize {
    total_fields(prog, sidx) + if has_vtable_slot(prog, sidx) { 2 } else { 1 }
}

/// Byte offset of the first field (16 with a vtable slot, 8 without).
pub(crate) fn field_base_offset(prog: &Program, sidx: usize) -> i64 {
    if has_vtable_slot(prog, sidx) {
        16
    } else {
        8
    }
}

/// The vtable slot names for `sidx`, in order. Interface methods come first
/// (in global interface order, padded with flint_unimplemented for interfaces
/// this class does not implement) so an interface method always sits at a
/// fixed slot across every implementing class. Then the class methods:
/// parent chain first (so an override keeps the parent's slot position),
/// then this class's own methods, skipping names already in the interface
/// part. Constructors and static methods are not in the vtable.
pub(crate) fn vtable_slots(prog: &Program, sidx: usize) -> Vec<String> {
    let mut slots = interface_part(prog);
    let mut order: Vec<String> = Vec::new();
    let mut cur = sidx;
    loop {
        for m in &prog.structs[cur].methods {
            // abstract methods keep their slot (as a flint_unimplemented stub)
            // so the slot index stays aligned across the hierarchy
            if m.is_static || m.is_ctor {
                continue;
            }
            if !slots.contains(&m.name) && !order.contains(&m.name) {
                order.push(m.name.clone());
            }
        }
        match parent_idx(prog, cur) {
            Some(p) => cur = p,
            None => break,
        }
    }
    slots.extend(order);
    slots
}

/// The interface part of every vtable: all interfaces' methods in global
/// interface order (deduped by name). Uniform across all classes, so an
/// interface method's slot is fixed.
fn interface_part(prog: &Program) -> Vec<String> {
    let mut slots: Vec<String> = Vec::new();
    for iface in &prog.interfaces {
        for m in &iface.methods {
            if !slots.contains(&m.name) {
                slots.push(m.name.clone());
            }
        }
    }
    slots
}

/// The fixed vtable slot for an interface method (in the interface part).
pub(crate) fn interface_slot(prog: &Program, method: &str) -> Option<usize> {
    interface_part(prog).iter().position(|n| n == method)
}

/// The class index that defines `name` for `sidx` (own, then the parent
/// chain). None when only an interface declares it (unimplemented) or it
/// doesn't exist.
pub(crate) fn find_method_def(prog: &Program, sidx: usize, name: &str) -> Option<(usize, usize)> {
    let s = &prog.structs[sidx];
    if let Some(mi) = s.methods.iter().position(|m| m.name == name) {
        return Some((sidx, mi));
    }
    if let Some(p) = parent_idx(prog, sidx) {
        return find_method_def(prog, p, name);
    }
    None
}

/// The mangled function name for vtable slot `name` of class `sidx` (the
/// most-derived concrete implementation). None when the most-derived
/// definition is an abstract (unimplemented) method.
pub(crate) fn vtable_slot_fn(prog: &Program, sidx: usize, name: &str) -> Option<String> {
    let (ci, mi) = find_method_def(prog, sidx, name)?;
    if prog.structs[ci].methods[mi].is_abstract {
        return None;
    }
    Some(format!("{}_{}", prog.structs[ci].name, name))
}

/// The .rodata symbol name of the vtable for `sidx`.
pub(crate) fn vtable_symbol(prog: &Program, sidx: usize) -> String {
    format!("vtable_{}", prog.structs[sidx].name)
}

/// True when `child` is a subtype of `parent_name` (a class it extends,
/// transitively, or an interface it implements, transitively).
pub(crate) fn is_subtype(prog: &Program, child: usize, parent_name: &str) -> bool {
    let s = &prog.structs[child];
    if s.extends.as_deref() == Some(parent_name) {
        return true;
    }
    if s.implements.iter().any(|i| i == parent_name) {
        return true;
    }
    if let Some(p) = parent_idx(prog, child) {
        return is_subtype(prog, p, parent_name);
    }
    false
}

/// The class indices that are subtypes of `target_name` (the class itself plus
/// all classes that extend it, or all classes that implement it if it is an
/// interface).
pub(crate) fn subclasses_of(prog: &Program, target_name: &str) -> Vec<usize> {
    prog.structs
        .iter()
        .enumerate()
        .filter(|(i, s)| s.name == target_name || is_subtype(prog, *i, target_name))
        .map(|(i, _)| i)
        .collect()
}
