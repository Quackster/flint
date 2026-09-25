use crate::ast::*;
use crate::error::{CompileError, CompileResult};
use crate::span::Span;
use crate::token::{Token, Tok};

/// Module keywords that may begin a qualified path (`sys.write(...)`).
pub const MODULES: &[&str] = &[
    "sys", "mem", "io", "str", "conv", "time", "rand", "env", "log", "b64", "math", "json",
];

fn is_type_keyword(k: &Tok) -> bool {
    matches!(
        k,
        Tok::KwByte
            | Tok::KwShort
            | Tok::KwInt
            | Tok::KwLong
            | Tok::KwChar
            | Tok::KwBoolean
            | Tok::KwString
    )
}

/// The fully qualified name of an item: `package.Name`, or just `Name` when
/// the package is empty (the default package). Used as the key in the name
/// maps and (with dots -> underscores) for symbol naming.
fn full_name(package: &str, name: &str) -> String {
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", package, name)
    }
}

/// Parse `Ident(.Ident)*` or `Ident(.Ident)*.*` starting at `start`.
/// Returns the joined name (dots) and the index just past the path.
/// Malformed paths yield `None` (best-effort; the real parse reports errors).
fn scan_dotted_path(toks: &[Token], start: usize) -> Option<(String, usize)> {
    let mut parts: Vec<String> = Vec::new();
    let mut k = start;
    match &toks.get(k).map(|t| t.kind.clone()).unwrap_or(Tok::Eof) {
        Tok::Ident(n) => {
            parts.push(n.clone());
            k += 1;
        }
        _ => return None,
    }
    while k < toks.len() && toks[k].kind == Tok::Dot {
        k += 1; // the '.'
        match &toks.get(k).map(|t| t.kind.clone()).unwrap_or(Tok::Eof) {
            Tok::Ident(n) => {
                parts.push(n.clone());
                k += 1;
            }
            Tok::Star => {
                parts.push("*".to_string());
                k += 1;
                break; // wildcard: nothing follows
            }
            _ => return None,
        }
    }
    Some((parts.join("."), k))
}

/// If `toks[j..]` begins a type, return how many tokens the type occupies.
/// A trailing `<...>` type-argument list (possibly nested) is included.
fn skip_type(j: usize, toks: &[Token]) -> Option<usize> {
    let k = toks.get(j)?.kind.clone();
    let base = match k {
        k if is_type_keyword(&k) => 1,
        Tok::Star => 1 + skip_type(j + 1, toks)?,
        Tok::Ident(_) => 1,
        _ => return None,
    };
    let mut total = base;
    let mut p = j + base;
    if toks.get(p).map(|t| &t.kind) == Some(&Tok::Lt) {
        total += 1; // the '<'
        p += 1;
        let mut depth = 1;
        while depth > 0 {
            let kk = toks.get(p)?.kind.clone();
            if kk == Tok::Lt {
                depth += 1;
            } else if kk == Tok::Gt {
                depth -= 1;
            }
            p += 1;
            total += 1;
        }
    }
    Some(total)
}

/// The name registry built by pre-scanning the source: class/interface/enum
/// names (by fully-qualified name) plus class field names and the import
/// list. Built once over all input files so that any file's expressions can
/// resolve names defined in another file.
#[derive(Clone)]
pub struct Prescan {
    class_names: std::collections::HashSet<String>,
    class_idx: std::collections::HashMap<String, usize>,
    class_fields: std::collections::HashMap<String, Vec<String>>,
    interface_names: std::collections::HashSet<String>,
    interface_idx: std::collections::HashMap<String, usize>,
    enum_names: std::collections::HashSet<String>,
    enum_idx: std::collections::HashMap<String, usize>,
    imports: Vec<String>,
}

/// Pre-scan for top-level definitions so expression parsing can resolve
/// `new Name(...)` and declaration types, and so class types get a stable
/// index (matching Program::structs). `boundaries` are token indices at which
/// the running `package` resets to the default (one per input file), so a
/// packageless file does not inherit the previous file's package.
pub fn prescan(toks: &[Token], boundaries: &std::collections::HashSet<usize>) -> Prescan {
    let mut class_names = std::collections::HashSet::new();
    let mut class_idx = std::collections::HashMap::new();
    let mut class_fields = std::collections::HashMap::new();
    let mut interface_names = std::collections::HashSet::new();
    let mut interface_idx = std::collections::HashMap::new();
    let mut enum_names = std::collections::HashSet::new();
    let mut enum_idx = std::collections::HashMap::new();
    let mut current_package = String::new();
    let mut imports = Vec::new();
    let mut k = 0;
    while k < toks.len() {
        if boundaries.contains(&k) {
            current_package = String::new();
        }
        // package Foo.Bar; -> update the current package for items that follow
        if toks[k].kind == Tok::Package {
            if let Some((pkg, _)) = scan_dotted_path(toks, k + 1) {
                current_package = pkg;
            }
        }
        // import com.other.Foo; / import com.other.*; -> record the import
        if toks[k].kind == Tok::Import {
            if let Some((path, _)) = scan_dotted_path(toks, k + 1) {
                imports.push(path);
            }
        }
        // interface Name -> register the interface (by full name)
        if toks[k].kind == Tok::Interface {
            if let Tok::Ident(n) = &toks.get(k + 1).map(|t| t.kind.clone()).unwrap_or(
                Tok::Eof,
            ) {
                let fqn = full_name(&current_package, n);
                if !interface_names.contains(&fqn) {
                    interface_idx.insert(fqn.clone(), interface_names.len());
                    interface_names.insert(fqn);
                }
            }
        }
        // enum Name -> register the enum (by full name)
        if toks[k].kind == Tok::Enum {
            if let Tok::Ident(n) = &toks.get(k + 1).map(|t| t.kind.clone()).unwrap_or(
                Tok::Eof,
            ) {
                let fqn = full_name(&current_package, n);
                if !enum_names.contains(&fqn) {
                    enum_idx.insert(fqn.clone(), enum_names.len());
                    enum_names.insert(fqn);
                }
            }
        }
        // class / abstract class -> register the class name
        let ck = if toks[k].kind == Tok::Class {
            Some(k)
        } else if toks[k].kind == Tok::Abstract
            && toks.get(k + 1).map(|t| &t.kind) == Some(&Tok::Class)
        {
            Some(k + 1)
        } else {
            None
        };
        if let Some(ck) = ck {
            if let Tok::Ident(n) = &toks.get(ck + 1).map(|t| t.kind.clone()).unwrap_or(
                Tok::Eof,
            ) {
                let fqn = full_name(&current_package, n);
                if !class_names.contains(&fqn) {
                    class_idx.insert(fqn.clone(), class_names.len());
                    class_names.insert(fqn.clone());
                    // collect field names: `class Name<T, U> { ... }`  skip methods too
                    let mut j = ck + 2;
                    // skip type parameters `<T, U>` between the name and `{`
                    if j < toks.len() && toks[j].kind == Tok::Lt {
                        while j < toks.len() && toks[j].kind != Tok::Gt {
                            j += 1;
                        }
                        if j < toks.len() && toks[j].kind == Tok::Gt {
                            j += 1;
                        }
                    }
                    if j < toks.len() && toks[j].kind == Tok::LBrace {
                        j += 1;
                        let mut names = Vec::new();
                        while j < toks.len() && toks[j].kind != Tok::RBrace {
                            // skip vis/accessor/static
                            let mut jj = j;
                            while jj < toks.len()
                                && matches!(
                                    toks[jj].kind,
                                    Tok::Private | Tok::Public | Tok::Static | Tok::Get | Tok::Set | Tok::GetSet
                                )
                            {
                                jj += 1;
                            }
                            // check for ctor (no return type)
                            if jj < toks.len() {
                                if let Tok::Ident(cname) = &toks[jj].kind {
                                    if cname == n
                                        && jj + 1 < toks.len()
                                        && toks[jj + 1].kind == Tok::LParen
                                    {
                                        // skip ctor method body
                                        j = jj + 1;
                                        while j < toks.len() && toks[j].kind != Tok::LBrace {
                                            j += 1;
                                        }
                                        if j < toks.len() && toks[j].kind == Tok::LBrace {
                                            let mut depth = 0;
                                            while j < toks.len() {
                                                if toks[j].kind == Tok::LBrace {
                                                    depth += 1;
                                                } else if toks[j].kind == Tok::RBrace {
                                                    depth -= 1;
                                                    if depth == 0 {
                                                        j += 1;
                                                        break;
                                                    }
                                                }
                                                j += 1;
                                            }
                                        }
                                        continue;
                                    }
                                }
                            }
                            // try type (including void for method)
                            let mut len_opt = None;
                            let mut is_void = false;
                            if jj < toks.len() && toks[jj].kind == Tok::Void {
                                len_opt = Some(1);
                                is_void = true;
                            } else if let Some(l) = skip_type(jj, toks) {
                                len_opt = Some(l);
                            }
                            if let Some(len) = len_opt {
                                let after_type = jj + len;
                                if after_type < toks.len() {
                                    if let Tok::Ident(fname) = &toks[after_type].kind {
                                        // lookahead: if after fname is '(' then it's a method, not field
                                        let after_fname = after_type + 1;
                                        let is_method = after_fname < toks.len()
                                            && toks[after_fname].kind == Tok::LParen;
                                        if !is_method {
                                            if !is_void {
                                                names.push(fname.clone());
                                            }
                                            j = after_fname;
                                            if j < toks.len() && toks[j].kind == Tok::Semicolon {
                                                j += 1;
                                            }
                                            continue;
                                        } else {
                                            // skip method signature: consume until '{' (body)
                                            // or ';' (abstract method, no body)
                                            j = after_fname;
                                            while j < toks.len()
                                                && toks[j].kind != Tok::LBrace
                                                && toks[j].kind != Tok::Semicolon
                                            {
                                                j += 1;
                                            }
                                            if j < toks.len() && toks[j].kind == Tok::Semicolon {
                                                j += 1;
                                            }
                                            if j < toks.len() && toks[j].kind == Tok::LBrace {
                                                let mut depth = 0;
                                                while j < toks.len() {
                                                    if toks[j].kind == Tok::LBrace {
                                                        depth += 1;
                                                    } else if toks[j].kind == Tok::RBrace {
                                                        depth -= 1;
                                                        if depth == 0 {
                                                            j += 1;
                                                            break;
                                                        }
                                                    }
                                                    j += 1;
                                                }
                                            }
                                            continue;
                                        }
                                    }
                                }
                                // fallback: treat as field-ish
                                j = after_type;
                                if j < toks.len() && toks[j].kind == Tok::Semicolon {
                                    j += 1;
                                }
                            } else {
                                j += 1;
                            }
                        }
                        class_fields.insert(fqn.clone(), names);
                    }
                }
            }
        }
        k += 1;
    }
    Prescan {
        class_names,
        class_idx,
        class_fields,
        interface_names,
        interface_idx,
        enum_names,
        enum_idx,
        imports,
    }
}

pub struct Parser<'a> {
    toks: &'a [Token],
    i: usize,
    /// The package path of the file section currently being parsed (set by
    /// the most recent `package` header). Empty = default package.
    current_package: String,
    /// All `import` paths, collected in the pre-scan (available to type
    /// resolution). Each is a fully qualified name or a wildcard (`pkg.*`).
    imports: Vec<String>,
    class_names: std::collections::HashSet<String>,
    class_idx: std::collections::HashMap<String, usize>,
    class_fields: std::collections::HashMap<String, Vec<String>>,
    interface_names: std::collections::HashSet<String>,
    interface_idx: std::collections::HashMap<String, usize>,
    enum_names: std::collections::HashSet<String>,
    enum_idx: std::collections::HashMap<String, usize>,
    // Innermost type-parameter scope: the `T`/`U` names of the generic
    // class/function currently being parsed, so `parse_type` can form `Ty::Param`.
    type_params: Vec<String>,
    // While parsing a generic function's *return type* (which precedes the
    // `<T>` list), allow an unknown identifier to become a `Ty::Param`; it is
    // validated against the parsed type-parameter list immediately after.
    allow_type_param: bool,
    // A `>>` is lexed as one `Shr` token; when it closes a nested type-argument
    // list, the second `>` is owed to the enclosing list. Tracks that debt.
    pending_gt: usize,
}

impl<'a> Parser<'a> {
    /// Build a parser that reuses a pre-built (global) name registry while
    /// parsing a single file's token slice with fresh (default-package)
    /// package state. Used for per-file parsing: the registry sees every
    /// file, but each file's definitions get their own package.
    pub fn with_registry(toks: &'a [Token], reg: &Prescan) -> Self {
        Self::from_prescan(toks, reg.clone())
    }

    fn from_prescan(toks: &'a [Token], p: Prescan) -> Self {
        Self {
            toks,
            i: 0,
            current_package: String::new(),
            imports: p.imports,
            class_names: p.class_names,
            class_idx: p.class_idx,
            class_fields: p.class_fields,
            interface_names: p.interface_names,
            interface_idx: p.interface_idx,
            enum_names: p.enum_names,
            enum_idx: p.enum_idx,
            type_params: Vec::new(),
            allow_type_param: false,
            pending_gt: 0,
        }
    }

    /// Resolve a (possibly short) class name to its fully qualified name,
    /// using the current package, the default package, and the import list.
    fn resolve_class(&self, name: &str) -> Option<String> {
        let fqn = full_name(&self.current_package, name);
        if self.class_names.contains(&fqn) {
            return Some(fqn);
        }
        if self.class_names.contains(name) {
            return Some(name.to_string());
        }
        // Specific import: `import a.b.C` makes `C` available (only when the
        // imported name is actually a class of the right kind).
        for imp in &self.imports {
            if !imp.ends_with(".*") && imp.rsplit('.').next() == Some(name)
                && self.class_names.contains(imp)
            {
                return Some(imp.clone());
            }
        }
        // Wildcard import: `import a.b.*` makes `a.b.C` available.
        for imp in &self.imports {
            if let Some(prefix) = imp.strip_suffix(".*") {
                let fqn = format!("{}.{}", prefix, name);
                if self.class_names.contains(&fqn) {
                    return Some(fqn);
                }
            }
        }
        None
    }

    /// Resolve a (possibly short) interface name to its fully qualified name.
    fn resolve_interface(&self, name: &str) -> Option<String> {
        let fqn = full_name(&self.current_package, name);
        if self.interface_names.contains(&fqn) {
            return Some(fqn);
        }
        if self.interface_names.contains(name) {
            return Some(name.to_string());
        }
        for imp in &self.imports {
            if !imp.ends_with(".*") && imp.rsplit('.').next() == Some(name)
                && self.interface_names.contains(imp)
            {
                return Some(imp.clone());
            }
        }
        for imp in &self.imports {
            if let Some(prefix) = imp.strip_suffix(".*") {
                let fqn = format!("{}.{}", prefix, name);
                if self.interface_names.contains(&fqn) {
                    return Some(fqn);
                }
            }
        }
        None
    }

    /// Resolve a (possibly short) enum name to its fully qualified name.
    fn resolve_enum(&self, name: &str) -> Option<String> {
        let fqn = full_name(&self.current_package, name);
        if self.enum_names.contains(&fqn) {
            return Some(fqn);
        }
        if self.enum_names.contains(name) {
            return Some(name.to_string());
        }
        for imp in &self.imports {
            if !imp.ends_with(".*") && imp.rsplit('.').next() == Some(name)
                && self.enum_names.contains(imp)
            {
                return Some(imp.clone());
            }
        }
        for imp in &self.imports {
            if let Some(prefix) = imp.strip_suffix(".*") {
                let fqn = format!("{}.{}", prefix, name);
                if self.enum_names.contains(&fqn) {
                    return Some(fqn);
                }
            }
        }
        None
    }

    /// Parse a `<T, U>` type-parameter list when present; else empty.
    fn parse_type_params(&mut self) -> CompileResult<Vec<String>> {
        if !self.at(&Tok::Lt) {
            return Ok(Vec::new());
        }
        self.bump(); // <
        let mut params = Vec::new();
        loop {
            let p = self.expect_ident()?;
            params.push(p);
            if self.at(&Tok::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect_gt()?;
        Ok(params)
    }

    /// Parse a `<int, string>` type-argument list (already past the `<`).
    fn parse_type_args(&mut self) -> CompileResult<Vec<Ty>> {
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if self.at(&Tok::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect_gt()?;
        Ok(args)
    }

    /// Parse a `<...>` type-argument list when the current token is `<`.
    fn parse_type_args_opt(&mut self) -> CompileResult<Vec<Ty>> {
        if !self.at(&Tok::Lt) {
            return Ok(Vec::new());
        }
        self.bump(); // <
        self.parse_type_args()
    }

    fn cur(&self) -> &Token {
        &self.toks[self.i]
    }

    fn at(&self, kind: &Tok) -> bool {
        std::mem::discriminant(&self.cur().kind) == std::mem::discriminant(kind)
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.i].clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }

    fn peek_kind(&self) -> Tok {
        self.cur().kind.clone()
    }

    fn next_kind(&self) -> Tok {
        self.toks
            .get(self.i + 1)
            .map(|t| t.kind.clone())
            .unwrap_or(Tok::Eof)
    }

    fn expect(&mut self, kind: &Tok, what: &str) -> CompileResult<Token> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(CompileError::new(
                self.cur().span,
                format!("expected {}, found {:?}", what, self.cur().kind),
            ))
        }
    }

    fn expect_ident(&mut self) -> CompileResult<String> {
        match &self.cur().kind {
            Tok::Ident(n) => {
                let n = n.clone();
                self.bump();
                Ok(n)
            }
            Tok::Get => {
                self.bump();
                Ok("get".to_string())
            }
            Tok::Set => {
                self.bump();
                Ok("set".to_string())
            }
            Tok::GetSet => {
                self.bump();
                Ok("getset".to_string())
            }
            _ => Err(CompileError::new(
                self.cur().span,
                format!("expected identifier, found {:?}", self.cur().kind),
            )),
        }
    }

    /// Consume the `>` that closes a type-argument / type-parameter list.
    /// A `>>` (lexed as `Shr`) closes the inner list and owes one `>` to the
    /// enclosing list (tracked in `pending_gt`).
    fn expect_gt(&mut self) -> CompileResult<Token> {
        if self.pending_gt > 0 {
            self.pending_gt -= 1;
            return Ok(Token {
                kind: Tok::Gt,
                span: self.cur().span,
            });
        }
        match self.cur().kind.clone() {
            Tok::Gt => Ok(self.bump()),
            Tok::Shr => {
                self.bump();
                self.pending_gt = 1;
                Ok(Token {
                    kind: Tok::Gt,
                    span: self.cur().span,
                })
            }
            other => Err(CompileError::new(
                self.cur().span,
                format!("expected '>', found {:?}", other),
            )),
        }
    }

    fn parse_type(&mut self) -> CompileResult<Ty> {
        let span = self.cur().span;
        // pointer type: '*' type
        if self.at(&Tok::Star) {
            self.bump();
            let _inner = self.parse_type()?;
            return Ok(Ty::Ptr);
        }
        // Qualified type name: `a.b.C` (an Ident followed by a Dot).
        if matches!(self.cur().kind, Tok::Ident(_))
            && self.toks.get(self.i + 1).map(|t| &t.kind) == Some(&Tok::Dot)
        {
            if let Some((path, end)) = scan_dotted_path(self.toks, self.i) {
                self.i = end;
                if self.class_names.contains(&path) {
                    let idx = self.class_idx.get(&path).copied().unwrap_or(0);
                    let args = self.parse_type_args_opt()?;
                    return Ok(if args.is_empty() {
                        Ty::Struct(idx)
                    } else {
                        Ty::Inst(idx, args)
                    });
                }
                if self.interface_names.contains(&path) {
                    let idx = self.interface_idx.get(&path).copied().unwrap_or(0);
                    return Ok(Ty::Interface(idx));
                }
                return Err(CompileError::new(
                    span,
                    format!("unknown type '{}'", path),
                ));
            }
        }
        let ty = match self.cur().kind.clone() {
            Tok::KwByte | Tok::KwShort | Tok::KwInt | Tok::KwLong | Tok::KwChar => {
                self.bump();
                Ty::Int
            }
            Tok::KwBoolean => {
                self.bump();
                Ty::Bool
            }
            Tok::KwString => {
                self.bump();
                Ty::Str
            }
            Tok::Ident(n) => {
                if let Some(fqn) = self.resolve_class(&n) {
                    // class type name; index matches Program::structs order
                    self.bump();
                    let idx = self.class_idx.get(&fqn).copied().unwrap_or(0);
                    let args = self.parse_type_args_opt()?;
                    if args.is_empty() {
                        Ty::Struct(idx)
                    } else {
                        Ty::Inst(idx, args)
                    }
                } else if let Some(fqn) = self.resolve_interface(&n) {
                    // interface type name; index matches Program::interfaces order
                    self.bump();
                    let idx = self.interface_idx.get(&fqn).copied().unwrap_or(0);
                    Ty::Interface(idx)
                } else if let Some(fqn) = self.resolve_enum(&n) {
                    // enum type name; index matches Program::enums order
                    self.bump();
                    let idx = self.enum_idx.get(&fqn).copied().unwrap_or(0);
                    Ty::Enum(idx)
                } else if self.type_params.contains(&n) || self.allow_type_param {
                    self.bump();
                    Ty::Param(n)
                } else {
                    return Err(CompileError::new(span, format!("unknown type '{}'", n)));
                }
            }
            Tok::Void => {
                return Err(CompileError::new(
                    span,
                    "void cannot be used as a variable type",
                ));
            }
            other => {
                return Err(CompileError::new(
                    span,
                    format!("expected type, found {:?}", other),
                ))
            }
        };
        Ok(ty)
    }

    /// Consume C-style trailing `[]` markers (`int a[]`, `int g[][]`) and
    /// return the array type. Without markers, `ty` is returned unchanged.
    fn array_suffix(&mut self, ty: Ty) -> CompileResult<Ty> {
        if self.at(&Tok::LBracket) {
            loop {
                self.expect(&Tok::LBracket, "'['")?;
                self.expect(&Tok::RBracket, "']' after '['")?;
                if !self.at(&Tok::LBracket) {
                    break;
                }
            }
            Ok(Ty::Array)
        } else {
            Ok(ty)
        }
    }

    /// True if `n` names a class or interface type.
    fn is_type_name(&self, n: &str) -> bool {
        self.resolve_class(n).is_some()
            || self.resolve_interface(n).is_some()
            || self.resolve_enum(n).is_some()
    }

    /// True if the token `off` positions ahead begins a type.
    fn is_type_at(&self, off: usize) -> bool {
        match self.toks.get(self.i + off).map(|t| &t.kind) {
            Some(k) if is_type_keyword(k) => true,
            Some(Tok::Star) => true,
            Some(Tok::Ident(n)) => self.is_type_name(n),
            _ => false,
        }
    }

    /// True if the token `off` positions ahead begins a castable type
    /// (a type keyword or a class/interface name; not a bare `*`).
    /// The index just past the closing `>` of a balanced `<...>` type-argument
    /// list starting at a `<` at `start`; None if unbalanced.
    fn type_args_end_at(&self, start: usize) -> Option<usize> {
        let mut depth = 0;
        let mut j = start;
        loop {
            match self.toks.get(j).map(|t| &t.kind) {
                Some(Tok::Lt) => depth += 1,
                Some(Tok::Shl) => depth += 2,
                Some(Tok::Gt) => depth -= 1,
                Some(Tok::Shr) => depth -= 2,
                _ => {}
            }
            j += 1;
            if depth == 0 {
                return Some(j);
            }
            if j >= self.toks.len() {
                return None;
            }
        }
    }

    /// True when the tokens at `self.i + off` form `( Type )` (a cast), as
    /// opposed to a parenthesized expression whose first tokens merely start
    /// with a type name, e.g. `(Math.abs(x) + 1)`.
    fn is_cast_type_at(&self, off: usize) -> bool {
        let j = self.i + off;
        match self.toks.get(j).map(|t| &t.kind) {
            Some(k) if is_type_keyword(k) => {}
            Some(Tok::Ident(_)) => {
                // Scan a qualified name (`a.b.C`).
                let mut k = j;
                while let Some(Tok::Dot) = self.toks.get(k + 1).map(|t| &t.kind) {
                    if !matches!(self.toks.get(k + 2).map(|t| &t.kind), Some(Tok::Ident(_))) {
                        return false;
                    }
                    k += 2;
                }
                let mut end = k + 1;
                if self.toks.get(k + 1).map(|t| &t.kind) == Some(&Tok::Lt) {
                    match self.type_args_end_at(k + 1) {
                        Some(e) => end = e,
                        None => return false,
                    }
                }
                return self.toks.get(end).map(|t| t.kind == Tok::RParen) == Some(true);
            }
            _ => return false,
        }
        self.toks.get(j + 1).map(|t| t.kind == Tok::RParen) == Some(true)
    }

    /// True if the current token starts a typed declaration (`Ty name = ...`).
    fn is_decl_start(&self) -> bool {
        match self.cur().kind.clone() {
            k if is_type_keyword(&k) => true,
            Tok::Star => self.is_type_at(1),
            Tok::Ident(n) => {
                // Qualified type name: `a.b.C x`. Scan the dotted path and
                // require the next token to be the declared variable name.
                if self.toks.get(self.i + 1).map(|t| &t.kind) == Some(&Tok::Dot) {
                    if let Some((_, end)) = scan_dotted_path(&self.toks, self.i) {
                        return match self.toks.get(end).map(|t| &t.kind) {
                            Some(Tok::Ident(_)) => true,
                            Some(Tok::Lt) => self.type_args_then_ident_at(end + 1),
                            _ => false,
                        };
                    } else {
                        return false;
                    }
                }
                if !self.is_type_name(&n) {
                    return false;
                }
                match self.next_kind() {
                    Tok::Ident(_) => true,
                    Tok::Lt => self.type_args_then_ident(),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// True if the tokens starting at `self.i + 1` (a `<`) form a balanced
    /// `<...>` type-argument list immediately followed by an identifier (the
    /// declared variable name), e.g. `Box<int> b` or `Box<Pair<int, int>> p`.
    /// `<<`/`>>` (lexed as `Shl`/`Shr`) count as two `</>`.
    fn type_args_then_ident(&self) -> bool {
        self.type_args_then_ident_at(self.i + 1)
    }

    /// Like `type_args_then_ident`, but starting at an explicit index (the
    /// position of the `<`).
    fn type_args_then_ident_at(&self, start: usize) -> bool {
        let mut depth = 0;
        let mut j = start; // the '<'
        loop {
            match self.toks.get(j).map(|t| &t.kind) {
                Some(Tok::Lt) => depth += 1,
                Some(Tok::Shl) => depth += 2,
                Some(Tok::Gt) => depth -= 1,
                Some(Tok::Shr) => depth -= 2,
                _ => {}
            }
            j += 1;
            if depth == 0 {
                return matches!(
                    self.toks.get(j).map(|t| &t.kind),
                    Some(Tok::Ident(_))
                );
            }
            if j >= self.toks.len() {
                return false;
            }
        }
    }

    /// True if the current token starts a generic function whose return type is
    /// a bare type parameter (a non-class identifier), e.g. `T identity<T>(T x)`.
    /// Such a signature is not caught by `is_decl_start` because the return
    /// type is not a known class.
    fn is_generic_func_start(&self) -> bool {
        let is_nonclass_ident = match self.toks.get(self.i).map(|t| &t.kind) {
            Some(Tok::Ident(n)) => !self.is_type_name(n),
            _ => false,
        };
        if !is_nonclass_ident {
            return false;
        }
        let name_is_ident = matches!(
            self.toks.get(self.i + 1).map(|t| &t.kind),
            Some(Tok::Ident(_))
        );
        let third = self.toks.get(self.i + 2).map(|t| &t.kind);
        name_is_ident && matches!(third, Some(Tok::Lt))
    }

    pub fn parse_program(&mut self) -> CompileResult<Program> {
        let mut structs = Vec::new();
        let mut funcs = Vec::new();
        let mut interfaces = Vec::new();
        let mut enums = Vec::new();
        let mut imports = Vec::new();
        while !self.at(&Tok::Eof) {
            if self.at(&Tok::Package) {
                self.bump(); // package
                match scan_dotted_path(self.toks, self.i) {
                    Some((pkg, end)) => {
                        self.current_package = pkg;
                        self.i = end;
                    }
                    None => {
                        return Err(CompileError::new(
                            self.cur().span,
                            "expected a dotted package path after 'package'",
                        ));
                    }
                }
                self.expect(&Tok::Semicolon, "a ';' after the package path")?;
            } else if self.at(&Tok::Import) {
                self.bump(); // import
                match scan_dotted_path(self.toks, self.i) {
                    Some((path, end)) => {
                        imports.push(path);
                        self.i = end;
                    }
                    None => {
                        return Err(CompileError::new(
                            self.cur().span,
                            "expected a dotted path after 'import'",
                        ));
                    }
                }
                self.expect(&Tok::Semicolon, "a ';' after the import path")?;
            } else if self.at(&Tok::Interface) {
                let mut d = self.parse_interface()?;
                d.package = self.current_package.clone();
                interfaces.push(d);
            } else if self.at(&Tok::Class) {
                let mut d = self.parse_class(false)?;
                d.package = self.current_package.clone();
                structs.push(d);
            } else if self.at(&Tok::Abstract) && self.next_kind() == Tok::Class {
                let mut d = self.parse_class(true)?;
                d.package = self.current_package.clone();
                structs.push(d);
            } else if self.at(&Tok::Enum) {
                let mut d = self.parse_enum()?;
                d.package = self.current_package.clone();
                enums.push(d);
            } else if self.at(&Tok::Async)
                || self.at(&Tok::Void)
                || self.is_decl_start()
                || self.is_generic_func_start()
            {
                let mut d = self.parse_func()?;
                d.package = self.current_package.clone();
                funcs.push(d);
            } else {
                return Err(CompileError::new(
                    self.cur().span,
                    format!(
                        "expected 'package', 'import', 'class', 'interface', or a function at top level, found {:?}",
                        self.cur().kind
                    ),
                ));
            }
        }
        Ok(Program { structs, funcs, interfaces, enums, imports })
    }

    fn parse_class(&mut self, is_abstract: bool) -> CompileResult<ClassDef> {
        let span = self.cur().span;
        if is_abstract {
            self.bump(); // abstract
        }
        self.bump(); // class
        let name = self.expect_ident()?;
        let type_params = self.parse_type_params()?;
        // `extends <Parent>`
        let extends = if self.at(&Tok::Extends) {
            self.bump();
            let p = self.expect_ident()?;
            Some(p)
        } else {
            None
        };
        // `implements A, B, ...`
        let mut implements = Vec::new();
        if self.at(&Tok::Implements) {
            self.bump();
            loop {
                let i = self.expect_ident()?;
                // Resolve to a fully qualified name so monomorphize can mangle
                // it with the *interface's* package (not the class's).
                implements.push(self.resolve_interface(&i).unwrap_or(i));
                if self.at(&Tok::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        self.expect(&Tok::LBrace, "'{'")?;
        let saved_tp = self.type_params.len();
        for p in &type_params {
            self.type_params.push(p.clone());
        }
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while !self.at(&Tok::RBrace) {
            // Check for constructor without return type: `Name ( params ) { ... }` or with vis
            // Peek visibility first
            let vis = if self.at(&Tok::Private) {
                self.bump();
                Vis::Private
            } else if self.at(&Tok::Public) {
                self.bump();
                Vis::Public
            } else {
                Vis::Public
            };
            // Check accessor for fields: get/set/getset
            let accessor = if self.at(&Tok::Get) {
                self.bump();
                Accessor::Get
            } else if self.at(&Tok::Set) {
                self.bump();
                Accessor::Set
            } else if self.at(&Tok::GetSet) {
                self.bump();
                Accessor::GetSet
            } else {
                Accessor::None
            };
            // `async` modifier (either order: `static async` / `async static`):
            // the method body runs on a worker thread when called; the call
            // expression evaluates to a std.Task.
            let mut is_async = if self.at(&Tok::Async) {
                self.bump();
                true
            } else {
                false
            };
            let is_static = if self.at(&Tok::Static) {
                self.bump();
                true
            } else {
                false
            };
            if !is_async && self.at(&Tok::Async) {
                self.bump();
                is_async = true;
            }
            // `abstract` modifier (the method body is `;`, detected in parse_method_rest)
            if self.at(&Tok::Abstract) {
                self.bump();
            }

            // Try to detect constructor: `ClassName(` directly (with optional vis already consumed)
            // But vis/accessor/static already consumed; need to check if current ident == class name and next is '('
            let is_ctor_candidate = match self.cur().kind.clone() {
                Tok::Ident(ref n) if n == &name => matches!(self.next_kind(), Tok::LParen),
                _ => false,
            };
            if is_ctor_candidate {
                if accessor != Accessor::None {
                    return Err(CompileError::new(
                        self.cur().span,
                        "constructor cannot have accessor `get`/`set`/`getset`",
                    ));
                }
                if is_async {
                    return Err(CompileError::new(
                        self.cur().span,
                        "async methods cannot be constructors",
                    ));
                }
                let m = self.parse_ctor(&name, vis, is_static)?;
                methods.push(m);
                continue;
            }

            // Otherwise we expect a type (or void) then name
            let mut ret = if self.at(&Tok::Void) {
                self.bump();
                Some(Ty::Void)
            } else if self.is_type_at(0) || is_type_keyword(&self.cur().kind) || matches!(self.cur().kind, Tok::Star | Tok::Ident(_)) {
                // Need to attempt parse_type — but if it's not a valid type, bail
                // Use peek: if star or type keyword or class ident
                // For robustness, try parse_type
                // We need to handle pointer types starting with *
                // Save index before
                let save = self.i;
                match self.parse_type() {
                    Ok(ty) => Some(ty),
                    Err(e) => {
                        self.i = save;
                        return Err(e);
                    }
                }
            } else {
                return Err(CompileError::new(
                    self.cur().span,
                    format!("expected type or method in class '{}', found {:?}", name, self.cur().kind),
                ));
            };
            // If we parsed a ctor candidate incorrectly, ret would be Ident class name as type; but ctor already handled
            // Now expect identifier name
            let member_name = self.expect_ident()?;
            // C-style array field: `int data[];`
            if self.at(&Tok::LBracket) {
                let t = ret.ok_or_else(|| CompileError::new(self.cur().span, "field cannot be void"))?;
                ret = Some(self.array_suffix(t)?);
            }
            // Lookahead to decide field vs method
            if self.at(&Tok::LParen) {
                // method
                if accessor != Accessor::None {
                    return Err(CompileError::new(
                        self.cur().span,
                        "methods cannot have `get`/`set`/`getset` accessors",
                    ));
                }
                // is_ctor: name matches the class and returns nothing. A ctor may
                // be written `void Name(...)` (explicit void) or `Name(...)` (no
                // return type); both are constructors per the language spec.
                let is_ctor = member_name == name
                    && ret.as_ref().map(|r| matches!(r, Ty::Void)).unwrap_or(true);
                let method = self.parse_method_rest(member_name, ret, vis, is_static, is_ctor, is_async)?;
                if is_async && method.is_abstract {
                    return Err(CompileError::new(
                        method.span,
                        "async methods cannot be abstract (the worker thread needs a body)",
                    ));
                }
                methods.push(method);
            } else if self.at(&Tok::Semicolon) {
                // field
                let fty = ret.ok_or_else(|| CompileError::new(self.cur().span, "field cannot be void"))?;
                if fty == Ty::Void {
                    return Err(CompileError::new(self.cur().span, "field cannot be void"));
                }
                self.bump(); // ';'
                fields.push(FieldDef {
                    span: self.cur().span,
                    name: member_name,
                    ty: fty,
                    vis,
                    accessor,
                    is_static,
                });
            } else {
                return Err(CompileError::new(
                    self.cur().span,
                    format!("expected '(' for method or ';' for field, found {:?}", self.cur().kind),
                ));
            }
        }
        self.expect(&Tok::RBrace, "'}'")?;
        self.type_params.truncate(saved_tp);
        Ok(ClassDef {
            span,
            package: String::new(),
            name,
            type_params,
            extends,
            implements,
            is_abstract,
            fields,
            methods,
        })
    }

    fn parse_ctor(&mut self, class_name: &str, vis: Vis, is_static: bool) -> CompileResult<MethodDef> {
        let span = self.cur().span;
        let name = self.expect_ident()?; // class name
        assert_eq!(name, class_name);
        if is_static {
            return Err(CompileError::new(span, "constructor cannot be static"));
        }
        self.expect(&Tok::LParen, "'(' after constructor name")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let pty = self.parse_type()?;
            let pname = self.expect_ident()?;
            let pty = self.array_suffix(pty)?;
            let pspan = self.cur().span;
            params.push((pname, Some(pty), pspan));
            if self.at(&Tok::Comma) {
                self.bump();
            }
        }
        self.expect(&Tok::RParen, "')'")?;
        let body = self.parse_block()?;
        Ok(MethodDef {
            span,
            name,
            vis,
            is_static: false,
            is_async: false,
            is_ctor: true,
            is_abstract: false,
            params,
            ret: Some(Ty::Void),
            body,
            type_params: Vec::new(),
        })
    }

    fn parse_method_rest(
        &mut self,
        name: String,
        ret: Option<Ty>,
        vis: Vis,
        is_static: bool,
        is_ctor: bool,
        is_async: bool,
    ) -> CompileResult<MethodDef> {
        let span = self.cur().span; // approximate
        self.expect(&Tok::LParen, "'(' after method name")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let pty = self.parse_type()?;
            let pname = self.expect_ident()?;
            let pty = self.array_suffix(pty)?;
            let pspan = self.cur().span;
            params.push((pname, Some(pty), pspan));
            if self.at(&Tok::Comma) {
                self.bump();
            }
        }
        self.expect(&Tok::RParen, "')'")?;
        // Abstract method: `;` instead of a `{ ... }` body.
        let (body, is_abstract) = if self.at(&Tok::Semicolon) {
            self.bump();
            (
                Block {
                    span: self.cur().span,
                    stmts: Vec::new(),
                },
                true,
            )
        } else {
            (self.parse_block()?, false)
        };
        Ok(MethodDef {
            span,
            name,
            vis,
            is_static,
            is_async,
            is_ctor,
            is_abstract,
            params,
            ret,
            body,
            type_params: Vec::new(),
        })
    }

    /// Parse `interface Name { <abstract method> ... }`. All methods are
    /// abstract (no bodies); no fields or constructors are allowed.
    fn parse_interface(&mut self) -> CompileResult<InterfaceDef> {
        let span = self.cur().span;
        self.bump(); // interface
        let name = self.expect_ident()?;
        self.expect(&Tok::LBrace, "'{'")?;
        let mut methods = Vec::new();
        while !self.at(&Tok::RBrace) {
            let vis = if self.at(&Tok::Private) {
                self.bump();
                Vis::Private
            } else if self.at(&Tok::Public) {
                self.bump();
                Vis::Public
            } else {
                Vis::Public
            };
            if self.at(&Tok::Abstract) {
                self.bump();
            }
            let ret = if self.at(&Tok::Void) {
                self.bump();
                Some(Ty::Void)
            } else {
                Some(self.parse_type()?)
            };
            let member_name = self.expect_ident()?;
            self.expect(&Tok::LParen, "'(' after interface method name")?;
            let mut params = Vec::new();
            while !self.at(&Tok::RParen) {
                let pty = self.parse_type()?;
                let pname = self.expect_ident()?;
                let pty = self.array_suffix(pty)?;
                let pspan = self.cur().span;
                params.push((pname, Some(pty), pspan));
                if self.at(&Tok::Comma) {
                    self.bump();
                }
            }
            self.expect(&Tok::RParen, "')'")?;
            self.expect(&Tok::Semicolon, "';' after interface method")?;
            methods.push(MethodDef {
                span,
                name: member_name,
                vis,
                is_static: false,
                is_async: false,
                is_ctor: false,
                is_abstract: true,
                type_params: Vec::new(),
                params,
                ret,
                body: Block {
                    span: self.cur().span,
                    stmts: Vec::new(),
                },
            });
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(InterfaceDef { span, package: String::new(), name, methods })
    }

    fn parse_enum(&mut self) -> CompileResult<EnumDef> {
        let span = self.cur().span;
        self.bump(); // enum
        let name = self.expect_ident()?;
        self.expect(&Tok::LBrace, "'{' after enum name")?;
        let mut variants = Vec::new();
        let mut next_val: i64 = 0;
        while !self.at(&Tok::RBrace) {
            let vname = self.expect_ident()?;
            let val = if self.at(&Tok::Assign) {
                self.bump(); // =
                let vt = self.bump();
                if let Tok::Int(v) = vt.kind {
                    next_val = v + 1;
                    v
                } else {
                    return Err(CompileError::new(
                        vt.span,
                        "expected an integer literal after '=' in enum",
                    ));
                }
            } else {
                let v = next_val;
                next_val += 1;
                v
            };
            variants.push((vname, val));
            if self.at(&Tok::Comma) {
                self.bump();
            }
        }
        self.expect(&Tok::RBrace, "'}' after enum variants")?;
        Ok(EnumDef { span, package: String::new(), name, variants })
    }

    fn parse_func(&mut self) -> CompileResult<FuncDef> {
        let span = self.cur().span;
        // `async` prefix: the body runs on a worker thread when called; the
        // call evaluates to a std.Task (join() for the result).
        let is_async = if self.at(&Tok::Async) {
            self.bump();
            true
        } else {
            false
        };
        // The return type may name a type parameter declared *after* the
        // function name (`T identity<T>(T x)`), so parse it leniently and
        // validate it against the type-parameter list once that is known.
        self.allow_type_param = true;
        let ret = if self.at(&Tok::Void) {
            self.bump();
            Some(Ty::Void)
        } else {
            Some(self.parse_type()?)
        };
        self.allow_type_param = false;
        let name = self.expect_ident()?;
        let type_params = self.parse_type_params()?;
        if let Some(Ty::Param(n)) = &ret {
            if !type_params.iter().any(|p| p == n) {
                return Err(CompileError::new(
                    span,
                    format!("unknown type parameter '{}'", n),
                ));
            }
        }
        let saved_tp = self.type_params.len();
        for p in &type_params {
            self.type_params.push(p.clone());
        }
        self.expect(&Tok::LParen, "'(' after function name")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let pty = self.parse_type()?;
            let pname = self.expect_ident()?;
            let pty = self.array_suffix(pty)?;
            let pspan = self.cur().span;
            params.push((pname, Some(pty), pspan));
            if self.at(&Tok::Comma) {
                self.bump();
            }
        }
        self.expect(&Tok::RParen, "')'")?;
        let body = self.parse_block()?;
        self.type_params.truncate(saved_tp);
        Ok(FuncDef {
            span,
            package: String::new(),
            name,
            type_params,
            is_async,
            params,
            ret,
            body,
        })
    }

    fn parse_block(&mut self) -> CompileResult<Block> {
        let span = self.cur().span;
        self.expect(&Tok::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) {
            self.parse_stmt(&mut stmts)?;
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(Block { span, stmts })
    }

    /// Parse `Ty name = expr`, optionally followed by ';' (a `for` init has
    /// its own separators).
    fn parse_decl(&mut self, expect_semi: bool) -> CompileResult<Stmt> {
        let span = self.cur().span;
        let ty = self.parse_type()?;
        let name = self.expect_ident()?;
        let ty = self.array_suffix(ty)?;
        self.expect(&Tok::Assign, "'=' after variable name")?;
        let value = self.parse_expr()?;
        if expect_semi {
            self.expect(&Tok::Semicolon, "';'")?;
        }
        Ok(Stmt::Decl {
            span,
            name,
            ty: Some(ty),
            value,
        })
    }

    /// Parse an expression, or `expr = expr`, as a statement (no trailing ';').
    /// Used for the init/update parts of `for`.
    fn parse_expr_or_assign(&mut self) -> CompileResult<Stmt> {
        let expr = self.parse_expr()?;
        if self.at(&Tok::Assign) {
            let span = self.cur().span;
            self.bump();
            let value = self.parse_expr()?;
            if !is_lvalue(&expr) {
                return Err(CompileError::new(
                    span,
                    "invalid assignment target",
                ));
            }
            Ok(Stmt::Assign {
                span,
                target: expr,
                value,
            })
        } else if let Some(op) = Self::compound_op(&self.cur().kind) {
            let span = self.cur().span;
            self.bump();
            let value = self.parse_expr()?;
            if !is_lvalue(&expr) {
                return Err(CompileError::new(
                    span,
                    "invalid compound assignment target",
                ));
            }
            Ok(Stmt::CompoundAssign {
                span,
                target: expr,
                op,
                value,
            })
        } else {
            Ok(Stmt::ExprStmt {
                span: expr_span(&expr),
                expr,
            })
        }
    }

    /// Parse `for (T x : a) { ... }` and desugar it to two statements:
    /// `int .rf_base = a;` followed by an index loop over `len(.rf_base)`.
    /// `ty` already includes any `[]` suffix on the loop variable (row type
    /// for multi-dimensional arrays).
    fn parse_range_for(
        &mut self,
        label: Option<String>,
        span: Span,
        name: String,
        ty: Ty,
    ) -> CompileResult<Vec<Stmt>> {
        self.bump(); // ':'
        let arr = self.parse_expr()?;
        self.expect(&Tok::RParen, "')' in 'for'")?;
        let body = self.parse_block()?;
        let base_name = ".rf_base";
        let idx_name = ".rf_i";
        // The desugared decls need distinct spans: the escape plan keys decl
        // decisions by decl-statement span. Offsets inside the `for` keyword
        // can never collide with a real statement's span.
        let base_span = Span::new(span.start + 1, span.end + 1);
        let idx_span = Span::new(span.start + 2, span.end + 2);
        let var_span = Span::new(span.start + 3, span.end + 3);
        let idx_ident = || Expr::Ident { span, name: idx_name.to_string() };
        let len_call = Expr::Call {
            span,
            callee: vec!["len".to_string()],
            type_args: Vec::new(),
            args: vec![Expr::Ident { span, name: base_name.to_string() }],
        };
        let cond = Expr::BinOp {
            span,
            op: BinOp::Lt,
            l: Box::new(idx_ident()),
            r: Box::new(len_call),
        };
        let update = Stmt::ExprStmt {
            span,
            expr: Expr::IncrDecr {
                span,
                e: Box::new(idx_ident()),
                inc: true,
                pre: false,
            },
        };
        let idx_decl = Stmt::Decl {
            span: idx_span,
            name: idx_name.to_string(),
            ty: Some(Ty::Int),
            value: Expr::Int { span: idx_span, value: 0 },
        };
        let elem = Expr::Index {
            span,
            base: Box::new(Expr::Ident { span, name: base_name.to_string() }),
            idx: Box::new(idx_ident()),
        };
        let var_decl = Stmt::Decl {
            span: var_span,
            name,
            ty: Some(ty),
            value: elem,
        };
        let mut body_stmts = vec![var_decl];
        body_stmts.extend(body.stmts);
        let for_stmt = Stmt::For {
            span,
            label,
            init: Some(Box::new(idx_decl)),
            cond: Some(Box::new(cond)),
            update: Some(Box::new(update)),
            body: Box::new(Block { span, stmts: body_stmts }),
        };
        let base_decl = Stmt::Decl {
            span: base_span,
            name: base_name.to_string(),
            ty: None,
            value: arr,
        };
        Ok(vec![base_decl, for_stmt])
    }

    fn parse_while(&mut self, label: Option<String>) -> CompileResult<Stmt> {
        let span = self.cur().span;
        self.bump(); // while
        self.expect(&Tok::LParen, "'(' after 'while'")?;
        let cond = self.parse_expr()?;
        self.expect(&Tok::RParen, "')' after 'while' condition")?;
        let body = self.parse_block()?;
        Ok(Stmt::While {
            span,
            label,
            cond: Box::new(cond),
            body: Box::new(body),
        })
    }

    fn parse_for(&mut self, mut label: Option<String>) -> CompileResult<Vec<Stmt>> {
        let span = self.cur().span;
        self.bump(); // for
        self.expect(&Tok::LParen, "'(' after 'for'")?;
        let init = if self.at(&Tok::Semicolon) {
            None
        } else if self.is_decl_start() {
            let ty = self.parse_type()?;
            let name = self.expect_ident()?;
            let ty = self.array_suffix(ty)?;
            if self.at(&Tok::Colon) {
                return self.parse_range_for(label.take(), span, name, ty);
            }
            self.expect(&Tok::Assign, "'=' after variable name")?;
            let value = self.parse_expr()?;
            Some(Stmt::Decl { span, name, ty: Some(ty), value })
        } else {
            Some(self.parse_expr_or_assign()?)
        };
        self.expect(&Tok::Semicolon, "';' in 'for'")?;
        let cond = if self.at(&Tok::Semicolon) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        self.expect(&Tok::Semicolon, "';' in 'for'")?;
        let update = if self.at(&Tok::RParen) {
            None
        } else {
            Some(Box::new(self.parse_expr_or_assign()?))
        };
        self.expect(&Tok::RParen, "')' in 'for'")?;
        let body = self.parse_block()?;
        Ok(vec![Stmt::For {
            span,
            label,
            init: init.map(Box::new),
            cond,
            update,
            body: Box::new(body),
        }])
    }

    /// Parse the statements of one `case`/`default` arm, stopping at the
    /// next `case`, `default`, or the closing `}`.
    fn parse_case_body(&mut self) -> CompileResult<Block> {
        let span = self.cur().span;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Case) && !self.at(&Tok::Default) {
            self.parse_stmt(&mut stmts)?;
        }
        Ok(Block { span, stmts })
    }

    fn parse_switch(&mut self) -> CompileResult<Stmt> {
        let span = self.cur().span;
        self.bump(); // switch
        self.expect(&Tok::LParen, "'(' after 'switch'")?;
        let target = self.parse_expr()?;
        self.expect(&Tok::RParen, "')' after 'switch' expression")?;
        self.expect(&Tok::LBrace, "'{' after 'switch'")?;
        let mut cases: Vec<(i64, Block)> = Vec::new();
        let mut default: Option<Box<Block>> = None;
        while !self.at(&Tok::RBrace) {
            if self.at(&Tok::Case) {
                let cspan = self.cur().span;
                self.bump();
                let v = match self.peek_kind() {
                    Tok::Int(v) => v,
                    other => {
                        return Err(CompileError::new(
                            self.cur().span,
                            format!(
                                "case value must be an integer literal, found {:?}",
                                other
                            ),
                        ))
                    }
                };
                self.bump();
                self.expect(&Tok::Colon, "':' after 'case' value")?;
                if cases.iter().any(|(w, _)| *w == v) {
                    return Err(CompileError::new(cspan, format!("duplicate case {}", v)));
                }
                cases.push((v, self.parse_case_body()?));
            } else if self.at(&Tok::Default) {
                let dspan = self.cur().span;
                self.bump();
                self.expect(&Tok::Colon, "':' after 'default'")?;
                if default.is_some() {
                    return Err(CompileError::new(dspan, "duplicate default"));
                }
                default = Some(Box::new(self.parse_case_body()?));
            } else {
                return Err(CompileError::new(
                    self.cur().span,
                    "expected 'case' or 'default' in 'switch' body",
                ));
            }
        }
        self.expect(&Tok::RBrace, "'}' after 'switch'")?;
        Ok(Stmt::Switch {
            span,
            target: Box::new(target),
            cases,
            default,
        })
    }

    /// Parse one statement (or a desugared run of them) and append to `out`.
    fn parse_stmt(&mut self, out: &mut Vec<Stmt>) -> CompileResult<()> {
        match self.peek_kind() {
            Tok::If => {
                let span = self.cur().span;
                self.bump();
                self.expect(&Tok::LParen, "'(' after 'if'")?;
                let cond = self.parse_expr()?;
                self.expect(&Tok::RParen, "')' after 'if' condition")?;
                let then = self.parse_block()?;
                let else_opt = if self.at(&Tok::Else) {
                    self.bump();
                    // `else if` -> nested block with a single if
                    if self.at(&Tok::If) {
                        let mut inner = Vec::new();
                        self.parse_stmt(&mut inner)?;
                        Some(Box::new(Block {
                            span: self.cur().span,
                            stmts: inner,
                        }))
                    } else {
                        Some(Box::new(self.parse_block()?))
                    }
                } else {
                    None
                };
                out.push(Stmt::If {
                    span,
                    cond: Box::new(cond),
                    then: Box::new(then),
                    else_opt,
                });
            }
            Tok::While => out.push(self.parse_while(None)?),
            Tok::For => out.extend(self.parse_for(None)?),
            Tok::Switch => out.push(self.parse_switch()?),
            Tok::Return => {
                let span = self.cur().span;
                self.bump();
                let value = if self.at(&Tok::Semicolon) {
                    None
                } else {
                    Some(Box::new(self.parse_expr()?))
                };
                self.expect(&Tok::Semicolon, "';'")?;
                out.push(Stmt::Return { span, value });
            }
            Tok::Break => {
                let span = self.cur().span;
                self.bump();
                let label = if matches!(self.peek_kind(), Tok::Ident(_)) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect(&Tok::Semicolon, "';' after 'break'")?;
                out.push(Stmt::Break { span, label });
            }
            Tok::Continue => {
                let span = self.cur().span;
                self.bump();
                let label = if matches!(self.peek_kind(), Tok::Ident(_)) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect(&Tok::Semicolon, "';' after 'continue'")?;
                out.push(Stmt::Continue { span, label });
            }
            Tok::Throw => {
                let span = self.cur().span;
                self.bump();
                let value = self.parse_expr()?;
                self.expect(&Tok::Semicolon, "';' after 'throw'")?;
                out.push(Stmt::Throw {
                    span,
                    value: Box::new(value),
                });
            }
            Tok::Try => {
                let span = self.cur().span;
                self.bump();
                let try_block = self.parse_block()?;
                self.expect(&Tok::Catch, "'catch' after 'try' block")?;
                self.expect(&Tok::LParen, "'(' after 'catch'")?;
                let catch_type = self.parse_type()?;
                let catch_var = self.expect_ident()?;
                self.expect(&Tok::RParen, "')' after catch param")?;
                let catch_block = self.parse_block()?;
                let finally = if self.at(&Tok::Finally) {
                    self.bump();
                    Some(Box::new(self.parse_block()?))
                } else {
                    None
                };
                out.push(Stmt::TryCatch {
                    span,
                    try_block: Box::new(try_block),
                    catch_type,
                    catch_var,
                    catch_block: Box::new(catch_block),
                    finally,
                });
            }
            _ => {
                // `name: while ...` / `name: for ...` — a labeled loop.
                if let Tok::Ident(ref n) = self.cur().kind {
                    if self.next_kind() == Tok::Colon
                        && matches!(
                            self.toks
                                .get(self.i + 2)
                                .map(|t| t.kind.clone()),
                            Some(Tok::While) | Some(Tok::For)
                        )
                    {
                        let label = n.clone();
                        self.bump(); // ident
                        self.bump(); // ':'
                        if self.at(&Tok::While) {
                            out.push(self.parse_while(Some(label))?);
                        } else {
                            out.extend(self.parse_for(Some(label))?);
                        }
                        return Ok(());
                    }
                }
                if self.is_decl_start() {
                    out.push(self.parse_decl(true)?);
                    return Ok(());
                }
                let expr = self.parse_expr()?;
                if self.at(&Tok::Assign) {
                    let span = self.cur().span;
                    self.bump();
                    let value = self.parse_expr()?;
                    self.expect(&Tok::Semicolon, "';'")?;
                    if !is_lvalue(&expr) {
                        return Err(CompileError::new(
                            span,
                            "invalid assignment target",
                        ));
                    }
                    out.push(Stmt::Assign {
                        span,
                        target: expr,
                        value,
                    });
                } else if let Some(op) = Self::compound_op(&self.cur().kind) {
                    let span = self.cur().span;
                    self.bump();
                    let value = self.parse_expr()?;
                    self.expect(&Tok::Semicolon, "';'")?;
                    if !is_lvalue(&expr) {
                        return Err(CompileError::new(
                            span,
                            "invalid compound assignment target",
                        ));
                    }
                    out.push(Stmt::CompoundAssign {
                        span,
                        target: expr,
                        op,
                        value,
                    });
                } else {
                    self.expect(&Tok::Semicolon, "';'")?;
                    out.push(Stmt::ExprStmt {
                        span: expr_span(&expr),
                        expr,
                    })
                }
            }
        }
        Ok(())
    }

    fn compound_op(tok: &Tok) -> Option<BinOp> {
        match tok {
            Tok::PlusEq => Some(BinOp::Add),
            Tok::MinusEq => Some(BinOp::Sub),
            Tok::StarEq => Some(BinOp::Mul),
            Tok::SlashEq => Some(BinOp::Div),
            _ => None,
        }
    }

    // ---- expressions (Pratt) ----

    fn parse_expr(&mut self) -> CompileResult<Expr> {
        let mut e = self.parse_binop(0)?;
        if self.at(&Tok::Question) {
            let span = self.cur().span;
            self.bump();
            let then = self.parse_binop(0)?;
            self.expect(&Tok::Colon, "':' in ternary")?;
            let els = self.parse_expr()?;
            let espan = expr_span(&e);
            e = Expr::Cond {
                span: espan.join(&span).join(&expr_span(&els)),
                cond: Box::new(e),
                then: Box::new(then),
                els: Box::new(els),
            };
        }
        Ok(e)
    }

    fn prec_of(tok: &Tok) -> Option<u8> {
        match tok {
            Tok::OrOr => Some(1),
            Tok::AndAnd => Some(2),
            Tok::EqEq | Tok::BangEq | Tok::Instanceof => Some(3),
            Tok::Lt | Tok::Gt | Tok::LtEq | Tok::GtEq => Some(4),
            Tok::Shl | Tok::Shr => Some(5),
            Tok::Amp => Some(6),
            Tok::Caret => Some(7),
            Tok::Pipe => Some(8),
            Tok::Plus | Tok::Minus => Some(9),
            Tok::Star | Tok::Slash | Tok::Percent => Some(10),
            _ => None,
        }
    }

    fn binop_of(tok: &Tok) -> Option<BinOp> {
        match tok {
            Tok::Plus => Some(BinOp::Add),
            Tok::Minus => Some(BinOp::Sub),
            Tok::Star => Some(BinOp::Mul),
            Tok::Slash => Some(BinOp::Div),
            Tok::Percent => Some(BinOp::Mod),
            Tok::Amp => Some(BinOp::BitAnd),
            Tok::AndAnd => Some(BinOp::And),
            Tok::Pipe => Some(BinOp::BitOr),
            Tok::OrOr => Some(BinOp::Or),
            Tok::Caret => Some(BinOp::BitXor),
            Tok::Shl => Some(BinOp::Shl),
            Tok::Shr => Some(BinOp::Shr),
            Tok::EqEq => Some(BinOp::Eq),
            Tok::BangEq => Some(BinOp::Ne),
            Tok::Lt => Some(BinOp::Lt),
            Tok::Gt => Some(BinOp::Gt),
            Tok::LtEq => Some(BinOp::Le),
            Tok::GtEq => Some(BinOp::Ge),
            _ => None,
        }
    }

    fn parse_binop(&mut self, min_prec: u8) -> CompileResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            // postfix is handled inside parse_unary/atom; here only binary ops
            let op_prec = match &self.cur().kind {
                k => Self::prec_of(k),
            };
            let prec = match op_prec {
                Some(p) if p >= min_prec => p,
                _ => break,
            };
            let cur_kind = self.peek_kind();
            let span = self.cur().span;
            self.bump();
            if cur_kind == Tok::Instanceof {
                // right operand is a type name, not an expression
                let ty = self.parse_type()?;
                let lspan = expr_span(&left);
                let span = lspan.join(&span);
                left = Expr::Instanceof {
                    span,
                    e: Box::new(left),
                    ty,
                };
            } else {
                let right = self.parse_binop(prec + 1)?;
                let lspan = expr_span(&left);
                let span = lspan.join(&span).join(&expr_span(&right));
                let op = Self::binop_of(&cur_kind).unwrap();
                left = Expr::BinOp {
                    span,
                    op,
                    l: Box::new(left),
                    r: Box::new(right),
                };
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> CompileResult<Expr> {
        let span = self.cur().span;
        let kind = self.peek_kind();
        let unop = match kind {
            Tok::Minus => Some(UnOp::Neg),
            Tok::Bang => Some(UnOp::Not),
            Tok::Star => Some(UnOp::Deref),
            Tok::Amp => Some(UnOp::Addr),
            Tok::At => Some(UnOp::FnAddr),
            _ => None,
        };
        if let Some(op) = unop {
            self.bump();
            let e = self.parse_unary()?;
            return match op {
                UnOp::Addr => Ok(Expr::AddrOf {
                    span: span.join(&expr_span(&e)),
                    e: Box::new(e),
                }),
                UnOp::Deref => Ok(Expr::Deref {
                    span: span.join(&expr_span(&e)),
                    e: Box::new(e),
                }),
                UnOp::FnAddr => Ok(Expr::FnAddr {
                    span: span.join(&expr_span(&e)),
                    e: Box::new(e),
                }),
                _ => Ok(Expr::UnOp {
                    span: span.join(&expr_span(&e)),
                    op,
                    e: Box::new(e),
                }),
            };
        }
        if kind == Tok::Incr || kind == Tok::Decr {
            let inc = kind == Tok::Incr;
            self.bump();
            let e = self.parse_unary()?;
            if !is_lvalue(&e) {
                return Err(CompileError::new(
                    span,
                    format!("'{}' requires a variable", if inc { "++" } else { "--" }),
                ));
            }
            return Ok(Expr::IncrDecr {
                span: span.join(&expr_span(&e)),
                e: Box::new(e),
                inc,
                pre: true,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> CompileResult<Expr> {
        let mut e = self.parse_atom()?;
        // Type args parsed for a `f<T>(...)` call; consumed by the next `(`.
        let mut call_type_args: Option<Vec<Ty>> = None;
        loop {
            if self.at(&Tok::LParen) {
                let span = self.cur().span;
                self.bump();
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) {
                    args.push(self.parse_expr()?);
                    if self.at(&Tok::Comma) {
                        self.bump();
                    }
                }
                self.expect(&Tok::RParen, "')'")?;
                // Determine if e is a field access -> method call
                if let Expr::Field { base, name, span: field_span } = e {
                    let span = field_span.join(&span);
                    e = Expr::MethodCall {
                        span,
                        base,
                        method: name,
                        args,
                    };
                } else {
                    let callee = expr_to_path(&e)?;
                    let span = expr_span(&e).join(&span);
                    e = Expr::Call {
                        span,
                        callee,
                        type_args: call_type_args.take().unwrap_or_default(),
                        args,
                    };
                }
            } else if self.at(&Tok::Lt) {
                // `f<T>(...)`: try to parse type args; if not followed by `(`,
                // this is a `<` comparison — back out and let binop handle it.
                let save = self.i;
                match self.parse_type_args_opt() {
                    Ok(args) if self.at(&Tok::LParen) => {
                        call_type_args = Some(args);
                    }
                    _ => {
                        self.i = save;
                        break;
                    }
                }
            } else if self.at(&Tok::Dot) {
                let span = self.cur().span;
                self.bump();
                let name = self.expect_ident()?;
                // Check if this is an enum variant access: EnumName.Variant
                if let Expr::Ident { name: ename, .. } = &e {
                    if self.resolve_enum(ename).is_some() {
                        e = Expr::EnumVariant {
                            span: expr_span(&e).join(&span),
                            enum_name: ename.clone(),
                            variant: name,
                        };
                        continue;
                    }
                }
                e = Expr::Field {
                    span: expr_span(&e).join(&span),
                    base: Box::new(e),
                    name,
                };
            } else if self.at(&Tok::LBracket) {
                let span = self.cur().span;
                self.bump();
                let idx = self.parse_expr()?;
                self.expect(&Tok::RBracket, "']'")?;
                e = Expr::Index {
                    span: expr_span(&e).join(&span),
                    base: Box::new(e),
                    idx: Box::new(idx),
                };
            } else if self.at(&Tok::Incr) || self.at(&Tok::Decr) {
                let span = self.cur().span;
                let inc = self.at(&Tok::Incr);
                self.bump();
                if !is_lvalue(&e) {
                    return Err(CompileError::new(
                        span,
                        format!(
                            "'{}' requires a variable",
                            if inc { "++" } else { "--" }
                        ),
                    ));
                }
                e = Expr::IncrDecr {
                    span: expr_span(&e).join(&span),
                    e: Box::new(e),
                    inc,
                    pre: false,
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_atom(&mut self) -> CompileResult<Expr> {
        let span = self.cur().span;
        match self.peek_kind() {
            Tok::Int(v) => {
                self.bump();
                Ok(Expr::Int { span, value: v })
            }
            Tok::True => {
                self.bump();
                Ok(Expr::Bool { span, value: true })
            }
            Tok::False => {
                self.bump();
                Ok(Expr::Bool { span, value: false })
            }
            Tok::This => {
                self.bump();
                Ok(Expr::This { span })
            }
            Tok::Null => {
                self.bump();
                Ok(Expr::Null { span })
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Str { span, value: s })
            }
            Tok::LParen => {
                // cast: `(Type) expr`
                if self.is_cast_type_at(1) {
                    self.bump(); // (
                    let ty = self.parse_type()?;
                    self.expect(&Tok::RParen, "')' after cast type")?;
                    let e = self.parse_unary()?;
                    let span = span.join(&expr_span(&e));
                    return Ok(Expr::Cast {
                        span,
                        ty,
                        e: Box::new(e),
                    });
                }
                self.bump();
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen, "')'")?;
                Ok(e)
            }
            Tok::Super => {
                self.bump();
                if self.at(&Tok::LParen) {
                    // `super(args)`: invoke the parent constructor on `this`
                    let span = self.cur().span;
                    self.bump(); // (
                    let mut args = Vec::new();
                    while !self.at(&Tok::RParen) {
                        args.push(self.parse_expr()?);
                        if self.at(&Tok::Comma) {
                            self.bump();
                        }
                    }
                    self.expect(&Tok::RParen, "')' after super arguments")?;
                    Ok(Expr::SuperCall { span, args })
                } else {
                    // `super` as a receiver: `super.field` / `super.method()`
                    Ok(Expr::SuperBase { span })
                }
            }
            Tok::LBracket => {
                self.bump();
                let mut elems = Vec::new();
                while !self.at(&Tok::RBracket) {
                    elems.push(self.parse_expr()?);
                    if self.at(&Tok::Comma) {
                        self.bump();
                    }
                }
                self.expect(&Tok::RBracket, "']'")?;
                Ok(Expr::ArrayLit { span, elems })
            }
            Tok::New => {
                self.bump();
                let name = if matches!(self.peek_kind(), Tok::Ident(_))
                    && self.next_kind() == Tok::Dot
                {
                    // qualified class name: a.b.Name
                    match scan_dotted_path(self.toks, self.i) {
                        Some((path, end)) => {
                            self.i = end;
                            path
                        }
                        None => {
                            return Err(CompileError::new(
                                self.cur().span,
                                "expected a dotted class name after 'new'",
                            ));
                        }
                    }
                } else {
                    self.expect_ident()?
                };
                let class_fqn = match self.resolve_class(&name) {
                    Some(fqn) => fqn,
                    None => {
                        return Err(CompileError::new(
                            span,
                            format!("unknown class '{}'", name),
                        ));
                    }
                };
                let type_args = self.parse_type_args_opt()?;
                self.expect(&Tok::LParen, "'(' after class name")?;
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) {
                    args.push(self.parse_expr()?);
                    if self.at(&Tok::Comma) {
                        self.bump();
                    }
                }
                self.expect(&Tok::RParen, "')' after class arguments")?;
                // For new, we now allow either positional field init or ctor args.
                // Determine if class has a constructor with matching arity later in codegen.
                // Here we just store args as fields tentatively; codegen will handle ctor dispatch.
                // To keep compat, if args len matches field count we treat as struct lit; otherwise treat as ctor call.
                let field_names = self
                    .class_fields
                    .get(&class_fqn)
                    .cloned()
                    .unwrap_or_default();
                // If class has methods that include a ctor, don't enforce field count here; allow any arity
                // Check if there's a ctor: we would need to know method list — but pre-scan doesn't have it.
                // So we relax: if args len == field count, treat as struct lit; else also treat as struct lit but with synthetic names _ctor_arg0 ?
                // Instead we store as StructLit with actual field names truncated/padded — codegen will handle ctor.
                // Simplest: always store with field names in order, but allow mismatch — codegen will decide.
                if args.len() == field_names.len() {
                    let fields: Vec<(String, Expr)> = field_names
                        .into_iter()
                        .zip(args)
                        .collect();
                    Ok(Expr::StructLit {
                        span,
                        name: class_fqn,
                        type_args,
                        fields,
                    })
                } else {
                    // Mismatched arity — treat as potential ctor call. Store with synthetic names
                    // so codegen can dispatch to constructor if one exists with this arity.
                    let fields: Vec<(String, Expr)> = args
                        .into_iter()
                        .enumerate()
                        .map(|(i, e)| (format!("_ctor_arg{}", i), e))
                        .collect();
                    Ok(Expr::StructLit {
                        span,
                        name: class_fqn,
                        type_args,
                        fields,
                    })
                }
            }
            Tok::Ident(name) => {
                let name = name.clone();
                self.bump();
                // module-qualified path?
                if MODULES.contains(&name.as_str()) && self.at(&Tok::Dot) {
                    let mut path = vec![name.clone()];
                    while self.at(&Tok::Dot) {
                        self.bump();
                        path.push(self.expect_ident()?);
                    }
                    let cspan = self.cur().span;
                    self.expect(&Tok::LParen, "'(' after module path")?;
                    let mut args = Vec::new();
                    while !self.at(&Tok::RParen) {
                        args.push(self.parse_expr()?);
                        if self.at(&Tok::Comma) {
                            self.bump();
                        }
                    }
                    self.expect(&Tok::RParen, "')'")?;
                    return Ok(Expr::Call {
                        span: span.join(&cspan),
                        callee: path,
                        type_args: Vec::new(),
                        args,
                    });
                }
                // plain identifier; postfix handled by caller
                Ok(Expr::Ident { span, name })
            }
            other => Err(CompileError::new(
                span,
                format!("expected expression, found {:?}", other),
            )),
        }
    }
}

pub fn expr_span(e: &Expr) -> Span {
    match e {
        Expr::Int { span, .. }
        | Expr::Bool { span, .. }
        | Expr::Str { span, .. }
        | Expr::Ident { span, .. }
        | Expr::This { span, .. }
        | Expr::Call { span, .. }
        | Expr::MethodCall { span, .. }
        | Expr::BinOp { span, .. }
        | Expr::UnOp { span, .. }
        | Expr::IncrDecr { span, .. }
        | Expr::AddrOf { span, .. }
        | Expr::Deref { span, .. }
        | Expr::FnAddr { span, .. }
        | Expr::Index { span, .. }
        | Expr::Field { span, .. }
        | Expr::StructLit { span, .. }
        | Expr::ArrayLit { span, .. }
        | Expr::Null { span, .. }
        | Expr::Cond { span, .. }
        | Expr::SuperBase { span, .. }
        | Expr::SuperCall { span, .. }
        | Expr::Cast { span, .. }
        | Expr::Instanceof { span, .. }
        | Expr::EnumVariant { span, .. } => *span,
    }
}

pub fn is_lvalue(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Ident { .. } | Expr::This { .. } | Expr::Index { .. } | Expr::Field { .. } | Expr::Deref { .. }
    )
}

/// Extract an identifier path from an expression that is a plain identifier.
fn expr_to_path(e: &Expr) -> CompileResult<Vec<String>> {
    match e {
        Expr::Ident { name, .. } => Ok(vec![name.clone()]),
        _ => Err(CompileError::new(
            expr_span(e),
            "expected a callable identifier",
        )),
    }
}
