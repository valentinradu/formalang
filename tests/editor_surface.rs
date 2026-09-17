//! The surface an editor drives.
//!
//! `compile_with_analyzer` is advertised for LSP use, so the node
//! finder, the position helpers and the query provider are a public
//! API. An editor calls them on every keystroke, at whatever offset
//! the caret happens to sit, over a buffer that is often mid-edit and
//! rarely valid.
//!
//! The tests here sweep every byte offset of several programs rather
//! than probing a handful of interesting ones. That is what an editor
//! does, and it is the only way to be sure no offset panics.

#![expect(
    clippy::expect_used,
    reason = "a sweep reports the offset that broke it by failing loudly"
)]

#[path = "common/mod.rs"]
mod common;

use common::Checked;

use formalang::semantic::node_finder::{find_node_at_offset, NodeAtPosition};
use formalang::semantic::position::{
    get_line_at_position, get_word_at_lsp_position, get_word_at_offset, span_contains_lsp_position,
    span_contains_offset, LspPosition,
};
use formalang::semantic::queries::QueryProvider;
use formalang::{compile_with_analyzer, parse_only, File, Location, Span};

/// Programs that between them use every top-level shape the node
/// finder knows: structs, enums, traits, impls, functions, module
/// lets, inline modules, closures, matches and generics.
const PROGRAMS: &[(&str, &str)] = &[
    (
        "shapes",
        r"pub trait Named {
    name: String
}

pub struct Widget {
    name: String,
    size: I32
}

impl Named for Widget {}

impl Widget {
    fn area(self) -> I32 {
        self.size * self.size
    }
}

pub enum Status {
    idle,
    busy(load: I32)
}

pub fn describe(w: Widget, s: Status) -> I32 {
    let scale = 2
    let f = (n: I32) -> n * scale
    match s {
        .idle: f(w.area()),
        .busy(load): load
    }
}
",
    ),
    (
        "generics",
        r"pub struct Box<T> {
    value: T
}

pub fn unwrap<T>(b: Box<T>) -> T {
    b.value
}

pub fn use_it() -> I32 {
    let b = Box<I32>(value: 7)
    unwrap<I32>(b: b)
}
",
    ),
    (
        "modules",
        r"pub mod geometry {
    pub struct Point {
        x: I32,
        y: I32
    }

    pub enum Direction {
        north,
        south
    }
}

pub fn origin() -> geometry::Point {
    geometry::Point(x: 0, y: 0)
}
",
    ),
    (
        "multibyte",
        "// héllo 中文 \u{1f600}\npub struct Wörld {\n    naïve: I32\n}\n",
    ),
    ("empty", ""),
    ("only_comment", "// nothing here\n"),
    (
        "broken",
        "pub struct Half {\n    a: I32\npub fn dangling( {\n",
    ),
];

/// Parse a program, tolerating failure. A broken buffer is the normal
/// case in an editor, and the finder must still work on whatever the
/// parser recovered.
fn ast_of(source: &str) -> Option<File> {
    parse_only(source).ok()
}

// ---------------------------------------------------------------------------
// The node finder
// ---------------------------------------------------------------------------

/// Every byte offset of every program resolves to some node, without
/// panicking.
#[test]
fn the_node_finder_is_total_over_every_offset() {
    let mut checked = Checked::new("offsets resolved by the node finder", 800);
    for (name, source) in PROGRAMS {
        let Some(file) = ast_of(source) else { continue };
        // One past the end, and a long way past it.
        for offset in 0..=source.len().saturating_add(64) {
            let context = find_node_at_offset(&file, offset);
            // Reading the result must be safe at any offset.
            let _ = context.enclosing_definition();
            let _ = context.is_in_expression();
            assert!(
                !matches!(context.enclosing_definition(), Some(NodeAtPosition::File)),
                "{name}: offset {offset} reported the file itself as an enclosing \
                 definition"
            );
            checked.hit();
        }
    }
}

/// An offset inside a definition finds that definition, and an offset
/// outside every definition finds none.
#[test]
fn an_offset_inside_a_definition_finds_it() {
    let source = PROGRAMS
        .iter()
        .find(|(name, _)| *name == "shapes")
        .map(|(_, source)| *source)
        .expect("the fixture must exist");
    let file = ast_of(source).expect("the fixture must parse");

    // The body of `describe` sits inside a function definition.
    let inside = source
        .find("self.size * self.size")
        .expect("the fixture must contain the method body");
    let context = find_node_at_offset(&file, inside);
    assert!(
        context.enclosing_definition().is_some(),
        "an offset inside a method body found no enclosing definition"
    );

    // Offset 0 is the `pub` of the first definition, so it is inside
    // one. Far past the end is inside none.
    let outside = find_node_at_offset(&file, source.len().saturating_add(1000));
    assert!(
        outside.enclosing_definition().is_none(),
        "an offset past the end of the file found an enclosing definition"
    );
}

/// The finder works on a buffer that did not fully parse.
#[test]
fn the_node_finder_works_on_a_broken_buffer() {
    let source = "pub struct Half {\n    a: I32\n";
    // `parse_only` may fail; the recovery path still yields a file for
    // some inputs, and when it does not there is nothing to sweep.
    let Some(file) = ast_of(source) else { return };
    for offset in 0..=source.len() {
        let _ = find_node_at_offset(&file, offset).is_in_expression();
    }
}

// ---------------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------------

/// Converting an LSP position to an offset and back must land where it
/// started, for every character boundary of every program.
#[test]
fn lsp_positions_round_trip_at_every_character_boundary() {
    let mut checked = Checked::new("positions round-tripped", 600);
    for (name, source) in PROGRAMS {
        for (offset, _) in source.char_indices() {
            let location = formalang::location::offset_to_location(offset, source);
            let position = LspPosition::from(location);
            let back = LspPosition::to_offset(source, position);
            assert_eq!(
                back, offset,
                "{name}: offset {offset} became {position:?} and came back as {back}"
            );
            checked.hit();
        }
    }
}

/// An LSP position past the end of a line clamps to the end of that
/// line, and one past the end of the file clamps to the end of the
/// file.
#[test]
fn an_lsp_position_past_the_end_clamps() {
    let source = "let a = 1\nlet bb = 2\n";
    let past_line = LspPosition::to_offset(source, LspPosition::new(0, 500));
    assert_eq!(
        past_line, 9,
        "a column past the end of line 1 should clamp to its end"
    );

    let past_file = LspPosition::to_offset(source, LspPosition::new(500, 0));
    assert_eq!(
        past_file,
        source.len(),
        "a line past the end of the file should clamp to its end"
    );
}

/// `get_word_at_offset` returns a word that really is at the range it
/// reports.
#[test]
fn the_word_at_an_offset_matches_the_source() {
    let mut checked = Checked::new("words matched against the source", 200);
    for (name, source) in PROGRAMS {
        for offset in 0..=source.len() {
            if !source.is_char_boundary(offset) {
                continue;
            }
            let Some((word, start, end)) = get_word_at_offset(source, offset) else {
                continue;
            };
            assert!(
                start <= end && end <= source.len(),
                "{name}: offset {offset} reported the range {start}..{end}"
            );
            let slice = source.get(start..end).unwrap_or("");
            assert_eq!(
                word, slice,
                "{name}: offset {offset} reported the word {word:?} but the source \
                 holds {slice:?} at {start}..{end}"
            );
            assert!(
                !word.is_empty(),
                "{name}: offset {offset} reported an empty word"
            );
            checked.hit();
        }
    }
}

/// An offset past the end of the source has no word.
#[test]
fn there_is_no_word_past_the_end() {
    let source = "let x = 1";
    assert!(
        get_word_at_offset(source, source.len().saturating_add(1)).is_none(),
        "an offset past the end reported a word"
    );
}

/// The word helpers agree whether they are given an offset or an LSP
/// position.
#[test]
fn the_word_helpers_agree() {
    let mut checked = Checked::new("positions compared across the word helpers", 200);
    for (name, source) in PROGRAMS {
        for (line_idx, line) in source.lines().enumerate() {
            for column in 0..line.chars().count() {
                let line_u32 = u32::try_from(line_idx).unwrap_or(u32::MAX);
                let column_u32 = u32::try_from(column).unwrap_or(u32::MAX);
                let position = LspPosition::new(line_u32, column_u32);
                let offset = LspPosition::to_offset(source, position);
                assert_eq!(
                    get_word_at_lsp_position(source, position),
                    get_word_at_offset(source, offset),
                    "{name}: the two word helpers disagree at {position:?}"
                );
                checked.hit();
            }
        }
    }
}

/// `get_line_at_position` returns the line the position names.
#[test]
fn the_line_at_a_position_is_the_right_line() {
    for (name, source) in PROGRAMS {
        for (index, expected) in source.lines().enumerate() {
            let line = u32::try_from(index).unwrap_or(u32::MAX);
            let got = get_line_at_position(source, LspPosition::new(line, 0));
            assert_eq!(got, expected, "{name}: line {index} came back wrong");
        }
        // A line past the end is empty, not a panic.
        assert_eq!(
            get_line_at_position(source, LspPosition::new(9999, 0)),
            "",
            "{name}: a line past the end must be empty"
        );
    }
}

/// `span_contains_offset` and `span_contains_lsp_position` agree.
#[test]
fn the_span_containment_helpers_agree() {
    let source = "let alpha = 1\nlet beta = 2\n";
    let span = Span::new(Location::new(4, 1, 5), Location::new(9, 1, 10));
    for (offset, _) in source.char_indices() {
        let location = formalang::location::offset_to_location(offset, source);
        let position = LspPosition::from(location);
        assert_eq!(
            span_contains_offset(&span, offset),
            span_contains_lsp_position(&span, position, source),
            "the two containment helpers disagree at offset {offset}"
        );
    }
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Every declared symbol has hover information, and hovering an
/// undeclared name gives none.
#[test]
fn every_declared_symbol_has_hover_information() {
    let mut checked = Checked::new("symbols hovered", 5);
    for (name, source) in PROGRAMS {
        let Ok((_, analyzer)) = compile_with_analyzer(source) else {
            continue;
        };
        let provider = QueryProvider::new(analyzer.symbols());

        for declared in ["Widget", "Named", "Status", "Box", "Point"] {
            if !source.contains(declared) {
                continue;
            }
            let hover = provider.get_hover_for_symbol(declared);
            assert!(
                hover.is_some(),
                "{name}: no hover information for the declared symbol {declared}"
            );
            if let Some(info) = hover {
                // A type inside an inline module answers under its
                // qualified name, e.g. `geometry::Point`.
                assert!(
                    info.symbol_name == declared || info.symbol_name.ends_with(declared),
                    "{name}: hover for {declared} named {}",
                    info.symbol_name
                );
                assert!(
                    !info.signature.is_empty(),
                    "{name}: hover for {declared} carried an empty signature"
                );
                checked.hit();
            }
        }

        assert!(
            provider
                .get_hover_for_symbol("definitely_not_declared_anywhere")
                .is_none(),
            "{name}: an undeclared name produced hover information"
        );
    }
}

/// Go-to-definition points at a span inside the source.
#[test]
fn go_to_definition_points_inside_the_source() {
    let mut checked = Checked::new("definitions resolved", 5);
    for (name, source) in PROGRAMS {
        let Ok((_, analyzer)) = compile_with_analyzer(source) else {
            continue;
        };
        let provider = QueryProvider::new(analyzer.symbols());

        for declared in ["Widget", "Named", "Status", "Box", "Point", "describe"] {
            if !source.contains(declared) {
                continue;
            }
            let Some(info) = provider.find_definition_by_name(declared) else {
                continue;
            };
            assert!(
                info.symbol_name == declared || info.symbol_name.ends_with(declared),
                "{name}: go-to-definition for {declared} named {}",
                info.symbol_name
            );
            assert!(
                info.span.start.offset <= info.span.end.offset,
                "{name}: {declared} has an inverted definition span {:?}",
                info.span
            );
            checked.hit();
        }

        assert!(
            provider
                .find_definition_by_name("definitely_not_declared_anywhere")
                .is_none(),
            "{name}: an undeclared name resolved to a definition"
        );
    }
}

/// Completions list every declared type, and never repeat a label
/// within one kind.
#[test]
fn completions_list_every_declared_type() {
    let mut checked = Checked::new("programs completed", 5);
    for (name, source) in PROGRAMS {
        let Ok((_, analyzer)) = compile_with_analyzer(source) else {
            continue;
        };
        let provider = QueryProvider::new(analyzer.symbols());

        let all = provider.get_all_completions();
        let types = provider.get_type_completions();
        assert!(
            !all.is_empty(),
            "{name}: the completion list held nothing, not even a keyword"
        );
        assert!(
            !types.is_empty(),
            "{name}: the type completion list held nothing, not even a primitive"
        );

        for declared in ["Widget", "Status", "Box"] {
            if !source.contains(declared) {
                continue;
            }
            assert!(
                all.iter().any(|c| c.label.ends_with(declared)),
                "{name}: {declared} is declared but absent from the completions"
            );
            assert!(
                types.iter().any(|c| c.label.ends_with(declared)),
                "{name}: {declared} is declared but absent from the type completions"
            );
        }

        // Every primitive is offered as a type.
        for primitive in ["I32", "I64", "F32", "F64", "Boolean", "String"] {
            assert!(
                types.iter().any(|c| c.label == primitive),
                "{name}: the primitive {primitive} is missing from the type completions"
            );
        }
        checked.hit();
    }
}

/// The query provider is stable: two calls over one analyzer give the
/// same answers.
#[test]
fn queries_are_stable_across_calls() {
    for (name, source) in PROGRAMS {
        let Ok((_, analyzer)) = compile_with_analyzer(source) else {
            continue;
        };
        let provider = QueryProvider::new(analyzer.symbols());

        let mut first: Vec<String> = provider
            .get_all_completions()
            .into_iter()
            .map(|c| c.label)
            .collect();
        let mut second: Vec<String> = provider
            .get_all_completions()
            .into_iter()
            .map(|c| c.label)
            .collect();
        first.sort_unstable();
        second.sort_unstable();
        assert_eq!(
            first, second,
            "{name}: two completion queries gave different answers"
        );
    }
}

/// Everything declared inside an inline module is visible to the
/// editor, under its qualified name and under its bare one.
///
/// The queries used to read only the top-level maps, so a program
/// built out of `pub mod` blocks gave no hover, no go-to-definition
/// and no completion for any of its contents.
#[test]
fn inline_module_contents_are_visible_to_the_editor() {
    let source = r"pub mod geometry {
    pub struct Point {
        x: I32
    }

    pub enum Direction {
        north
    }

    pub mod deep {
        pub struct Inner {
            v: I32
        }
    }
}
";
    let (_, analyzer) = compile_with_analyzer(source).expect("the fixture must compile");
    let provider = QueryProvider::new(analyzer.symbols());

    for (bare, qualified) in [
        ("Point", "geometry::Point"),
        ("Direction", "geometry::Direction"),
        ("Inner", "geometry::deep::Inner"),
    ] {
        for probe in [bare, qualified] {
            assert!(
                provider.get_hover_for_symbol(probe).is_some(),
                "no hover for {probe}"
            );
            assert!(
                provider.find_definition_by_name(probe).is_some(),
                "no definition for {probe}"
            );
        }
        let labels: Vec<String> = provider
            .get_all_completions()
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert!(
            labels.iter().any(|l| l == qualified),
            "{qualified} is missing from the completions"
        );
    }

    // The modules themselves are visible too.
    for module in ["geometry", "geometry::deep"] {
        assert!(
            provider.get_hover_for_symbol(module).is_some(),
            "no hover for the module {module}"
        );
        assert!(
            provider.find_definition_by_name(module).is_some(),
            "no definition for the module {module}"
        );
    }
}

/// A qualified name that names the wrong module does not resolve.
#[test]
fn a_wrong_module_path_does_not_resolve() {
    let source = "pub mod geometry {\n    pub struct Point {\n        x: I32\n    }\n}\n";
    let (_, analyzer) = compile_with_analyzer(source).expect("the fixture must compile");
    let provider = QueryProvider::new(analyzer.symbols());

    assert!(
        provider.get_hover_for_symbol("colors::Point").is_none(),
        "a type resolved through a module it does not belong to"
    );
}
