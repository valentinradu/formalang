//! The integration-test suite, as one binary.
//!
//! Cargo compiles every `tests/*.rs` file as its own crate, with its own
//! link step. At eighty files that link dominates the build, and every
//! mutation-sweep copy pays it again. This file declares each test file
//! as a module instead, so the suite links once.
//!
//! To add a test file: put it in `tests/suite/`, and add a `mod` line
//! below. Use `crate::common::` for the shared helpers.

mod common;

mod api_contracts;
mod cli;
mod closure_captures;
mod closure_conv;
mod complete_fv_no_error_types;
mod concurrency;
mod conformance;
mod cross_module;
mod cross_module_adversarial;
mod cross_module_inline;
mod default_parameters;
mod defunctionalise;
mod depth_ladder;
mod destructuring;
mod destructuring_closure_annotations;
mod diagnostics_from_source;
mod dictionary_keys;
mod differential;
mod doc_comments_and_lines;
mod docs_examples;
mod editor_surface;
mod error_paths;
mod examples_ir_anomalies;
mod execution_matrix;
mod fold_width;
mod frontend_invariants;
mod function_visibility;
mod indentation;
mod inferred_enum_context;
mod integration;
mod ir;
mod ir_spans;
mod ir_verifier;
mod lexer_correctness;
mod lsp;
mod matrix_answers;
mod metamorphic;
mod method_matrix;
mod mined_modules;
mod module_resolution;
mod near_miss;
mod no_internal_errors;
mod numeric_literal_precision;
mod numeric_semantics;
mod operator_matrix;
mod optimised_answers;
mod parser_edge_cases;
mod pass_adversarial;
mod pass_contracts;
mod pathological_inputs;
mod performance;
mod proptest_frontend;
mod renaming;
mod reporting;
mod reporting_colour;
mod reporting_every_variant;
mod resolve_refs;
mod run_examples;
mod semantic;
mod semantic_analysis;
mod semantic_edge_cases;
mod semantic_validation;
mod sequences;
mod shape_equivalence;
mod snapshots;
mod string_builtins;
mod test_ast_and_token_helpers;
mod test_dce_visitor;
mod test_error_variants;
mod test_extern;
mod test_functional_semantic;
mod test_gaps;
mod test_impl_trait;
mod test_ir_fold2;
mod test_ir_lower2;
mod test_ir_lower3;
mod test_ir_lower_modules;
mod test_no_ui;
mod test_node_finder;
mod test_overloading;
mod test_param_conventions;
mod test_pipeline;
mod test_position_coverage;
mod test_semantic_coverage;
mod test_semantic_coverage2;
mod test_semantic_coverage3;
mod test_semantic_coverage4;
mod test_serde_stability;
mod test_trait_methods;
mod tests_assert_something;
mod token_tests;
mod trait_value_positions;
mod type_agreement;
mod type_matrix;
mod unary_operands;
