//! Near-miss mutation of valid programs.
//!
//! Each accepted conformance case and each example is a program that
//! compiles. This file applies one small edit to such a program, of a kind
//! where the correct verdict is certain: the program must be rejected. A
//! rename of a variant to a name that no enum declares, an argument that
//! is missing or unknown, a literal of the wrong type, a call of a method
//! that does not exist. The rejection must also name the mistake: an
//! internal compiler error is not a correct answer.
//!
//! A mutant that compiles marks a hole in the checks of the compiler. The
//! failure message of each test lists every such mutant, with the file and
//! the edit.
//!
//! The engine works on the text. It masks comments and string literals,
//! reads the declarations of the file (enums, functions, structs), and
//! edits only in code. It skips a shape when it cannot be sure of the
//! verdict: an overloaded function, a positional argument, a generic
//! field.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "a text scanner counts bytes; each loop condition checks its bound"
)]

use crate::common::Checked;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The most mutants of one class taken from one file.
const PER_FILE: usize = 6;

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// Every program that compiles today: the conformance cases that expect
/// `run` or `compile`, and the examples.
fn corpus() -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(root, &path, out);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("fv") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push((rel.to_string_lossy().into_owned(), source));
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut all = Vec::new();
    walk(&root, &root.join("tests").join("conformance"), &mut all);
    walk(&root, &root.join("examples"), &mut all);
    all.into_iter()
        .filter(|(name, source)| {
            let accepts = !name.contains("conformance")
                || source.lines().any(|l| {
                    let l = l.trim();
                    l == "// expect: run" || l == "// expect: compile"
                });
            accepts && formalang::compile_to_ir(source).is_ok()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The scanner
// ---------------------------------------------------------------------------

/// A source file, with a mask that is true for each byte of code (not a
/// comment and not a string literal).
struct Text {
    src: String,
    code: Vec<bool>,
}

impl Text {
    fn new(src: &str) -> Self {
        let b = src.as_bytes();
        let mut code = vec![true; b.len()];
        let mut i = 0;
        while i < b.len() {
            if b[i..].starts_with(b"//") {
                while i < b.len() && b[i] != b'\n' {
                    code[i] = false;
                    i += 1;
                }
            } else if b[i..].starts_with(b"/*") {
                while i < b.len() && !b[i..].starts_with(b"*/") {
                    code[i] = false;
                    i += 1;
                }
                let end = (i + 2).min(b.len());
                while i < end {
                    code[i] = false;
                    i += 1;
                }
            } else if b[i..].starts_with(b"\"\"\"") {
                code[i] = false;
                code[i + 1] = false;
                code[i + 2] = false;
                i += 3;
                while i < b.len() && !b[i..].starts_with(b"\"\"\"") {
                    code[i] = false;
                    i += 1;
                }
                let end = (i + 3).min(b.len());
                while i < end {
                    code[i] = false;
                    i += 1;
                }
            } else if b[i] == b'"' {
                code[i] = false;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        code[i] = false;
                        i += 1;
                    }
                    code[i] = false;
                    i += 1;
                }
                if i < b.len() {
                    code[i] = false;
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        Self {
            src: src.to_string(),
            code,
        }
    }

    fn bytes(&self) -> &[u8] {
        self.src.as_bytes()
    }

    fn is_code(&self, i: usize) -> bool {
        self.code.get(i).copied().unwrap_or(false)
    }

    /// The end of the identifier that starts at `i`.
    fn ident_end(&self, i: usize) -> usize {
        let b = self.bytes();
        let mut j = i;
        while j < b.len() && is_ident(b[j]) && self.is_code(j) {
            j += 1;
        }
        j
    }

    /// Each identifier in code: `(start, end)`.
    fn idents(&self) -> Vec<(usize, usize)> {
        let b = self.bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if self.is_code(i) && is_ident_start(b[i]) && (i == 0 || !is_ident(b[i - 1])) {
                let end = self.ident_end(i);
                out.push((i, end));
                i = end;
            } else {
                i += 1;
            }
        }
        out
    }

    /// The index of the bracket that closes the one at `open`.
    fn matching(&self, open: usize) -> Option<usize> {
        let b = self.bytes();
        let mut depth = 0_i32;
        let mut i = open;
        while i < b.len() {
            if self.is_code(i) {
                match b[i] {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        None
    }

    /// The ranges between the commas at depth 0 in `start..end`.
    fn split_commas(&self, start: usize, end: usize) -> Vec<(usize, usize)> {
        let b = self.bytes();
        let mut out = Vec::new();
        let mut depth = 0_i32;
        let mut from = start;
        let mut i = start;
        while i < end {
            if self.is_code(i) {
                match b[i] {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth -= 1,
                    b',' if depth == 0 => {
                        out.push((from, i));
                        from = i + 1;
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        out.push((from, end));
        out.retain(|&(s, e)| !self.src[s..e].trim().is_empty());
        out
    }

    /// The first byte of code at or after `i` that is not a space.
    fn skip_space(&self, mut i: usize) -> usize {
        let b = self.bytes();
        while i < b.len() && (b[i].is_ascii_whitespace() || !self.is_code(i)) {
            i += 1;
        }
        i
    }

    /// The last byte of code before `i` that is not a space.
    fn prev_code_byte(&self, i: usize) -> Option<(usize, u8)> {
        let b = self.bytes();
        let mut j = i;
        while j > 0 {
            j -= 1;
            if self.is_code(j) && !b[j].is_ascii_whitespace() {
                return Some((j, b[j]));
            }
        }
        None
    }

    /// The word that ends just before `i`, if any.
    fn prev_word(&self, i: usize) -> Option<&str> {
        let (end, byte) = self.prev_code_byte(i)?;
        if !is_ident(byte) {
            return None;
        }
        let b = self.bytes();
        let mut start = end;
        while start > 0 && is_ident(b[start - 1]) {
            start -= 1;
        }
        Some(&self.src[start..=end])
    }

    fn replace(&self, start: usize, end: usize, with: &str) -> String {
        format!("{}{}{}", &self.src[..start], with, &self.src[end..])
    }
}

const fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

// ---------------------------------------------------------------------------
// The declarations
// ---------------------------------------------------------------------------

/// One parameter or field: its label, its type, and whether it may be left
/// out.
#[derive(Clone, Debug)]
struct Slot {
    label: String,
    ty: String,
    required: bool,
}

#[derive(Default)]
struct Decls {
    enums: HashSet<String>,
    variants: HashSet<String>,
    /// How many times each function or method name is declared.
    fn_count: HashMap<String, usize>,
    /// The parameters of each free function or method, by name. `None`
    /// when a parameter has no label.
    fns: HashMap<String, Option<Vec<Slot>>>,
    /// The fields of each struct with no type parameters.
    structs: HashMap<String, Vec<Slot>>,
    /// Every struct name, generic or not.
    struct_names: HashSet<String>,
}

/// Split `label: Type = default` into a slot. `None` for a slot with no
/// label.
fn parse_slot(text: &str) -> Option<Slot> {
    let t = text.trim();
    let t = t
        .strip_prefix("mut ")
        .or_else(|| t.strip_prefix("sink "))
        .unwrap_or(t)
        .trim();
    let colon = t.find(':')?;
    let label = t[..colon].trim();
    if label.is_empty() || !label.bytes().all(is_ident) {
        return None;
    }
    let rest = &t[colon + 1..];
    let (ty, has_default) = split_default(rest);
    Some(Slot {
        label: label.to_string(),
        ty: ty.trim().to_string(),
        required: !has_default && !ty.trim().ends_with('?'),
    })
}

/// Find a `=` that starts a default: not part of `==`, `!=`, `<=`, `>=`.
fn split_default(rest: &str) -> (&str, bool) {
    let b = rest.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'=' {
            continue;
        }
        let before = if i > 0 { b[i - 1] } else { b' ' };
        let after = b.get(i + 1).copied().unwrap_or(b' ');
        if after != b'=' && !matches!(before, b'=' | b'!' | b'<' | b'>') {
            return (&rest[..i], true);
        }
    }
    (rest, false)
}

fn declarations(t: &Text) -> Decls {
    let mut d = Decls::default();
    d.variants.insert("some".to_string());
    d.variants.insert("none".to_string());
    let idents = t.idents();
    for (k, &(s, e)) in idents.iter().enumerate() {
        let word = &t.src[s..e];
        let Some(&(ns, ne)) = idents.get(k + 1) else {
            continue;
        };
        let name = t.src[ns..ne].to_string();
        match word {
            "enum" => {
                d.enums.insert(name.clone());
                let Some(open) = t.src[ne..].find('{').map(|o| o + ne) else {
                    continue;
                };
                let Some(close) = t.matching(open) else {
                    continue;
                };
                for (vs, ve) in t.split_commas(open + 1, close) {
                    for line in t.src[vs..ve].lines() {
                        let line = line.trim();
                        let v: String = line
                            .bytes()
                            .take_while(|&c| is_ident(c))
                            .map(char::from)
                            .collect();
                        if !v.is_empty() && !line.starts_with("//") {
                            d.variants.insert(v);
                        }
                    }
                }
            }
            "fn" => {
                *d.fn_count.entry(name.clone()).or_insert(0) += 1;
                let mut i = t.skip_space(ne);
                if t.bytes().get(i) == Some(&b'<') {
                    let mut depth = 0;
                    while i < t.src.len() {
                        match t.bytes()[i] {
                            b'<' => depth += 1,
                            b'>' => {
                                depth -= 1;
                                if depth == 0 {
                                    i += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    i = t.skip_space(i);
                }
                if t.bytes().get(i) != Some(&b'(') {
                    continue;
                }
                let Some(close) = t.matching(i) else {
                    continue;
                };
                let mut slots = Some(Vec::new());
                for (ps, pe) in t.split_commas(i + 1, close) {
                    let p = t.src[ps..pe].trim();
                    let p = p
                        .strip_prefix("mut ")
                        .or_else(|| p.strip_prefix("sink "))
                        .unwrap_or(p)
                        .trim();
                    if p == "self" {
                        continue;
                    }
                    match (parse_slot(p), slots.as_mut()) {
                        (Some(slot), Some(list)) => list.push(slot),
                        _ => slots = None,
                    }
                }
                d.fns.insert(name, slots);
            }
            "struct" => {
                d.struct_names.insert(name.clone());
                let i = t.skip_space(ne);
                if t.bytes().get(i) != Some(&b'{') {
                    continue;
                }
                let Some(close) = t.matching(i) else {
                    continue;
                };
                let fields: Vec<Slot> = t
                    .split_commas(i + 1, close)
                    .into_iter()
                    .filter_map(|(fs, fe)| parse_slot(&t.src[fs..fe]))
                    .collect();
                d.structs.insert(name, fields);
            }
            _ => {}
        }
    }
    d
}

// ---------------------------------------------------------------------------
// The call sites
// ---------------------------------------------------------------------------

/// A call `name(label: value, ...)` whose callee the declarations know.
struct Call {
    open: usize,
    close: usize,
    /// Each argument: its range, its label, and the range of its value.
    args: Vec<(usize, usize, String, usize, usize)>,
    slots: Vec<Slot>,
    kind: &'static str,
    name: String,
}

fn calls(t: &Text, d: &Decls) -> Vec<Call> {
    let mut out = Vec::new();
    for (s, e) in t.idents() {
        let name = &t.src[s..e];
        if t.bytes().get(e) != Some(&b'(') {
            continue;
        }
        if let Some((_, byte)) = t.prev_code_byte(s) {
            if byte == b'.' || (byte == b':' && t.src[..s].ends_with("::")) {
                continue;
            }
        }
        if matches!(t.prev_word(s), Some("fn")) {
            continue;
        }
        let (slots, kind) = if let Some(fields) = d.structs.get(name) {
            if d.fns.contains_key(name) {
                continue;
            }
            (fields.clone(), "struct")
        } else if let Some(Some(params)) = d.fns.get(name) {
            if d.fn_count.get(name).copied().unwrap_or(0) != 1 {
                continue;
            }
            (params.clone(), "function")
        } else {
            continue;
        };
        let Some(close) = t.matching(e) else {
            continue;
        };
        let mut args = Vec::new();
        let mut all_labelled = true;
        for (as_, ae) in t.split_commas(e + 1, close) {
            let text = &t.src[as_..ae];
            let lead = text.len() - text.trim_start().len();
            let ls = as_ + lead;
            let le = t.ident_end(ls);
            let after = t.skip_space(le);
            let is_label = le > ls
                && t.bytes().get(after) == Some(&b':')
                && t.bytes().get(after + 1) != Some(&b':');
            if !is_label {
                all_labelled = false;
                break;
            }
            let label = t.src[ls..le].to_string();
            if !slots.iter().any(|sl| sl.label == label) {
                all_labelled = false;
                break;
            }
            args.push((as_, ae, label, after + 1, ae));
        }
        if !all_labelled {
            continue;
        }
        out.push(Call {
            open: e,
            close,
            args,
            slots,
            kind,
            name: name.to_string(),
        });
    }
    out
}

fn is_literal(v: &str) -> Option<&'static str> {
    let v = v.trim();
    if v == "true" || v == "false" {
        return Some("Boolean");
    }
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') && !v.starts_with("\"\"\"") {
        return Some("String");
    }
    let digits = v.strip_prefix('-').unwrap_or(v);
    if digits.bytes().next().is_some_and(|b| b.is_ascii_digit())
        && digits
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
    {
        return Some("Number");
    }
    None
}

/// A literal that no value of `ty` accepts, when `ty` is a primitive or an
/// optional of one.
fn wrong_literal_for(ty: &str) -> Option<&'static str> {
    match ty.trim().trim_end_matches('?') {
        "I32" | "I64" | "F32" | "F64" | "Boolean" => Some("\"zz\""),
        "String" => Some("true"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The mutations
// ---------------------------------------------------------------------------

/// One mutant: what the edit was, and the new source.
type Mutant = (String, String);

fn rename_leading_dot_variants(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    for (s, e) in t.idents() {
        let name = &t.src[s..e];
        if !d.variants.contains(name) || s == 0 || t.bytes()[s - 1] != b'.' || !t.is_code(s - 1) {
            continue;
        }
        let Some((_, before)) = t.prev_code_byte(s - 1) else {
            continue;
        };
        if is_ident(before) || matches!(before, b')' | b']' | b'}' | b'?' | b'"') {
            continue;
        }
        let line = t.src[..s].lines().count();
        out.push((
            format!("line {line}: `.{name}` renamed to `.{name}_zz`"),
            t.replace(s, e, &format!("{name}_zz")),
        ));
    }
    out
}

fn rename_qualified_variants(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    let idents = t.idents();
    for w in idents.windows(2) {
        let (es, ee) = w[0];
        let (vs, ve) = w[1];
        let enum_name = &t.src[es..ee];
        let variant = &t.src[vs..ve];
        if !d.enums.contains(enum_name) || ee + 1 != vs || t.bytes()[ee] != b'.' {
            continue;
        }
        if es > 0 && t.bytes()[es - 1] == b'.' {
            continue;
        }
        let line = t.src[..vs].lines().count();
        out.push((
            format!("line {line}: `{enum_name}.{variant}` renamed to `{enum_name}.{variant}_zz`"),
            t.replace(vs, ve, &format!("{variant}_zz")),
        ));
    }
    out
}

fn drop_required_arguments(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    for call in calls(t, d) {
        for (k, (_, _, label, _, _)) in call.args.iter().enumerate() {
            let required = call.slots.iter().any(|s| &s.label == label && s.required);
            if !required {
                continue;
            }
            let kept: Vec<&str> = call
                .args
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .map(|(_, (s, e, _, _, _))| t.src[*s..*e].trim())
                .collect();
            let line = t.src[..call.open].lines().count();
            out.push((
                format!(
                    "line {line}: required `{label}` dropped from a call of {} `{}`",
                    call.kind, call.name
                ),
                t.replace(call.open + 1, call.close, &kept.join(", ")),
            ));
        }
    }
    out
}

fn add_unknown_arguments(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    for call in calls(t, d) {
        let inner = t.src[call.open + 1..call.close].trim();
        let new_inner = if inner.is_empty() {
            "zz_unknown: 1".to_string()
        } else {
            format!("{inner}, zz_unknown: 1")
        };
        let line = t.src[..call.open].lines().count();
        out.push((
            format!(
                "line {line}: `zz_unknown: 1` added to a call of {} `{}`",
                call.kind, call.name
            ),
            t.replace(call.open + 1, call.close, &new_inner),
        ));
    }
    out
}

fn wrong_argument_literals(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    for call in calls(t, d) {
        for (_, _, label, vs, ve) in &call.args {
            if is_literal(&t.src[*vs..*ve]).is_none() {
                continue;
            }
            let Some(slot) = call.slots.iter().find(|s| &s.label == label) else {
                continue;
            };
            let Some(wrong) = wrong_literal_for(&slot.ty) else {
                continue;
            };
            let line = t.src[..*vs].lines().count();
            out.push((
                format!(
                    "line {line}: `{label}` of {} `{}` (a {}) given {wrong}",
                    call.kind, call.name, slot.ty
                ),
                t.replace(*vs, *ve, &format!(" {wrong}")),
            ));
        }
    }
    out
}

fn wrong_let_literals(t: &Text) -> Vec<Mutant> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in t.src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let body = line.trim_end();
        let lead = body.len() - body.trim_start().len();
        if !t.is_code(start + lead) {
            continue;
        }
        let trimmed = body.trim_start();
        let rest = trimmed
            .strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("let ");
        let Some(rest) = rest else {
            continue;
        };
        let rest = rest.strip_prefix("mut ").unwrap_or(rest);
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let Some(eq) = rest.find(" = ") else {
            continue;
        };
        if eq < colon {
            continue;
        }
        let ty = rest[colon + 1..eq].trim();
        let value = rest[eq + 3..].trim();
        if value.contains("//") || is_literal(value).is_none() {
            continue;
        }
        let Some(wrong) = wrong_literal_for(ty) else {
            continue;
        };
        let value_start = start + body.len() - value.len();
        let number = t.src[..start].lines().count() + 1;
        out.push((
            format!("line {number}: `{value}` in a `{ty}` let replaced by {wrong}"),
            t.replace(value_start, value_start + value.len(), wrong),
        ));
    }
    out
}

fn unknown_methods(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    for (s, e) in t.idents() {
        if s == 0 || t.bytes()[s - 1] != b'.' || t.bytes().get(e) != Some(&b'(') {
            continue;
        }
        let Some((_, before)) = t.prev_code_byte(s - 1) else {
            continue;
        };
        if !(is_ident(before) || matches!(before, b')' | b']' | b'"')) {
            continue;
        }
        let name = &t.src[s..e];
        if d.variants.contains(name) {
            continue;
        }
        let line = t.src[..s].lines().count();
        out.push((
            format!("line {line}: method `.{name}(` renamed to `.zz_{name}(`"),
            t.replace(s, e, &format!("zz_{name}")),
        ));
    }
    out
}

fn unknown_annotation_types(t: &Text, d: &Decls) -> Vec<Mutant> {
    let mut out = Vec::new();
    let idents = t.idents();
    for w in idents.windows(3) {
        let (ks, ke) = w[0];
        let (ts, te) = w[2];
        if &t.src[ks..ke] != "let" {
            continue;
        }
        let between = &t.src[w[1].1..ts];
        if between.trim() != ":" {
            continue;
        }
        let ty = &t.src[ts..te];
        let known = d.struct_names.contains(ty)
            || d.enums.contains(ty)
            || matches!(ty, "I32" | "I64" | "F32" | "F64" | "Boolean" | "String");
        if !known {
            continue;
        }
        let line = t.src[..ts].lines().count();
        out.push((
            format!("line {line}: annotation `{ty}` renamed to `Zz{ty}`"),
            t.replace(ts, te, &format!("Zz{ty}")),
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// The runner
// ---------------------------------------------------------------------------

/// Apply one class of mutation to the whole corpus, and return the
/// mutants that were not rejected properly.
fn survivors(class: &'static str, mutate: &dyn Fn(&Text, &Decls) -> Vec<Mutant>) -> Vec<String> {
    let mut checked = Checked::new(class, 20);
    let mut out = Vec::new();
    for (name, source) in corpus() {
        let text = Text::new(&source);
        let decls = declarations(&text);
        for (edit, mutant) in mutate(&text, &decls).into_iter().take(PER_FILE) {
            match formalang::compile_to_ir(&mutant) {
                Ok(_) => out.push(format!("{name}: {edit}: compiles")),
                Err(errors) => {
                    if errors
                        .iter()
                        .any(|e| matches!(e, formalang::CompilerError::InternalError { .. }))
                    {
                        out.push(format!("{name}: {edit}: an internal error: {errors:?}"));
                    }
                }
            }
            checked.hit();
        }
    }
    out
}

fn assert_no_survivors(class: &str, found: &[String]) {
    assert!(
        found.is_empty(),
        "{} mutant(s) of the class `{class}` were not rejected properly:\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn an_unknown_leading_dot_variant_is_rejected() {
    let found = survivors("leading-dot variants", &rename_leading_dot_variants);
    assert_no_survivors("an unknown `.variant`", &found);
}

#[test]
fn an_unknown_qualified_variant_is_rejected() {
    let found = survivors("qualified variants", &rename_qualified_variants);
    assert_no_survivors("an unknown `Enum.variant`", &found);
}

#[test]
fn a_missing_required_argument_is_rejected() {
    let found = survivors("dropped arguments", &drop_required_arguments);
    assert_no_survivors("a missing required argument", &found);
}

#[test]
fn an_unknown_named_argument_is_rejected() {
    let found = survivors("added arguments", &add_unknown_arguments);
    assert_no_survivors("an unknown named argument", &found);
}

#[test]
fn an_argument_literal_of_another_type_is_rejected() {
    let found = survivors("argument literals", &wrong_argument_literals);
    assert_no_survivors("an argument literal of another type", &found);
}

#[test]
fn a_let_literal_of_another_type_is_rejected() {
    let found = survivors("let literals", &|t: &Text, _: &Decls| wrong_let_literals(t));
    assert_no_survivors("a let literal of another type", &found);
}

#[test]
fn an_unknown_method_is_rejected() {
    let found = survivors("method calls", &unknown_methods);
    assert_no_survivors("an unknown method", &found);
}

#[test]
fn an_unknown_annotation_type_is_rejected() {
    let found = survivors("annotation types", &unknown_annotation_types);
    assert_no_survivors("an unknown annotation type", &found);
}

/// The engine itself: on a program with each shape, each class finds a
/// mutant, and none of them touches a comment or a string. The verdict of
/// the compiler on a mutant is the job of the tests above, not of this one.
#[test]
fn the_mutation_engine_finds_each_shape() {
    let source = "\
// .red Point(x: 1) p.len() let q: I32 = 1
pub enum Colour { red, green }
pub struct Point { x: I32, y: I32 }
fn area(p: Point, scale: I32) -> I32 { p.x * p.y * scale }

pub fn run_checks() {
    let c: Colour = .red
    let d = Colour.green
    let n: I32 = 3
    let s = \".green area(p: 1)\"
    let a = area(p: Point(x: 1, y: 2), scale: 2)
    assert(condition: s.len() > 0 && a == n && c != d)
}
";
    assert!(
        formalang::compile_to_ir(source).is_ok(),
        "the sample must compile"
    );
    let t = Text::new(source);
    let d = declarations(&t);
    let counts = [
        ("leading dot", rename_leading_dot_variants(&t, &d)),
        ("qualified", rename_qualified_variants(&t, &d)),
        ("dropped", drop_required_arguments(&t, &d)),
        ("added", add_unknown_arguments(&t, &d)),
        ("argument literal", wrong_argument_literals(&t, &d)),
        ("let literal", wrong_let_literals(&t)),
        ("method", unknown_methods(&t, &d)),
        ("annotation", unknown_annotation_types(&t, &d)),
    ];
    for (class, mutants) in &counts {
        assert!(!mutants.is_empty(), "the class `{class}` found no mutant");
        for (edit, mutant) in mutants {
            assert!(
                mutant.starts_with("// .red Point(x: 1) p.len() let q: I32 = 1\n"),
                "`{class}` edited the comment: {edit}"
            );
            assert!(
                mutant.contains("\".green area(p: 1)\""),
                "`{class}` edited the string: {edit}"
            );
            assert_ne!(mutant.as_str(), source, "`{class}` changed nothing: {edit}");
        }
    }
    let dropped: Vec<&String> = counts[2].1.iter().map(|(e, _)| e).collect();
    assert_eq!(dropped.len(), 4, "x, y, p and scale: {dropped:?}");
}
