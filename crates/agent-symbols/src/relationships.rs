use crate::extractor::find_name;
use crate::languages::Language;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Calls,
    Imports,
    Inherits,
    Implements,
}

impl std::fmt::Display for RelationshipKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Calls => write!(f, "calls"),
            Self::Imports => write!(f, "imports"),
            Self::Inherits => write!(f, "inherits"),
            Self::Implements => write!(f, "implements"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipConfidence {
    Extracted,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub kind: RelationshipKind,
    pub source_symbol: Option<String>,
    pub target: String,
    pub confidence: RelationshipConfidence,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipQueryCoverage {
    pub calls: bool,
    pub imports: bool,
    pub inherits: bool,
    pub implements: bool,
}

#[derive(Debug, Clone, Copy)]
struct QuerySpec {
    kind: RelationshipKind,
    source: &'static str,
    csharp_base_classification: bool,
}

pub fn extract_relationships_from_tree(
    tree: &Tree,
    source: &str,
    language: Language,
) -> Result<Vec<ExtractedRelationship>> {
    let ts_language = language.ts_language();
    let mut relationships = Vec::new();
    for spec in query_specs(language) {
        let query = Query::new(&ts_language, spec.source)
            .with_context(|| format!("invalid {} relationship query for {language}", spec.kind))?;
        run_query(
            &query,
            *spec,
            tree.root_node(),
            source,
            language,
            &mut relationships,
        )?;
    }
    relationships.sort_by(|left, right| {
        (
            left.start_byte,
            left.end_byte,
            left.kind,
            left.source_symbol.as_deref(),
            left.target.as_str(),
        )
            .cmp(&(
                right.start_byte,
                right.end_byte,
                right.kind,
                right.source_symbol.as_deref(),
                right.target.as_str(),
            ))
    });
    relationships.dedup();
    Ok(relationships)
}

pub fn coverage(language: Language) -> RelationshipQueryCoverage {
    let specs = query_specs(language);
    RelationshipQueryCoverage {
        calls: specs
            .iter()
            .any(|spec| spec.kind == RelationshipKind::Calls),
        imports: specs
            .iter()
            .any(|spec| spec.kind == RelationshipKind::Imports),
        inherits: specs
            .iter()
            .any(|spec| spec.kind == RelationshipKind::Inherits),
        implements: specs
            .iter()
            .any(|spec| spec.kind == RelationshipKind::Implements)
            || specs.iter().any(|spec| spec.csharp_base_classification),
    }
}

fn run_query(
    query: &Query,
    spec: QuerySpec,
    root: Node<'_>,
    source: &str,
    language: Language,
    output: &mut Vec<ExtractedRelationship>,
) -> Result<()> {
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(10_000);
    let mut matches = cursor.matches(query, root, source.as_bytes());
    while let Some(query_match) = matches.next() {
        let mut explicit_source = None;
        let mut site = None;
        let mut targets = Vec::new();
        for capture in query_match.captures {
            match capture_names[capture.index as usize] {
                "source" => explicit_source = node_text(capture.node, source),
                "site" => site = Some(capture.node),
                "target" => targets.push(capture.node),
                _ => {}
            }
        }
        for target_node in targets {
            let Some(raw_target) = node_text(target_node, source) else {
                continue;
            };
            let target = normalize_target(spec.kind, &raw_target);
            if target.is_empty() || skip_target_node(target_node) {
                continue;
            }
            let site = site.unwrap_or(target_node);
            let source_symbol = explicit_source
                .clone()
                .or_else(|| owning_symbol_name(site, source, language));
            let (kind, confidence) = if spec.csharp_base_classification {
                classify_csharp_base(&target)
            } else {
                (spec.kind, RelationshipConfidence::Extracted)
            };
            output.push(ExtractedRelationship {
                kind,
                source_symbol,
                target,
                confidence,
                start_line: site.start_position().row + 1,
                end_line: site.end_position().row + 1,
                start_byte: site.start_byte(),
                end_byte: site.end_byte(),
            });
        }
    }
    drop(matches);
    if cursor.did_exceed_match_limit() {
        anyhow::bail!(
            "{} relationship query exceeded the match limit for {language}",
            spec.kind
        );
    }
    Ok(())
}

fn owning_symbol_name(mut node: Node<'_>, source: &str, language: Language) -> Option<String> {
    while let Some(parent) = node.parent() {
        if language.symbol_node_kinds().contains(&parent.kind()) {
            if let Some(name) = find_name(parent, source, &language) {
                return Some(name);
            }
        }
        node = parent;
    }
    None
}

fn node_text(node: Node<'_>, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes())
        .ok()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_target(kind: RelationshipKind, raw: &str) -> String {
    let mut target = raw.trim();
    if kind == RelationshipKind::Imports {
        target = target.trim_matches(['"', '\'']);
    }
    target.trim().to_owned()
}

fn skip_target_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "access_specifier" | "comment" | "type_arguments" | "argument_list" | "keyword_argument"
    )
}

fn classify_csharp_base(target: &str) -> (RelationshipKind, RelationshipConfidence) {
    let simple = target
        .rsplit('.')
        .next()
        .unwrap_or(target)
        .split('<')
        .next()
        .unwrap_or(target);
    let looks_like_interface = simple
        .strip_prefix('I')
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_uppercase);
    if looks_like_interface {
        (
            RelationshipKind::Implements,
            RelationshipConfidence::Ambiguous,
        )
    } else {
        (
            RelationshipKind::Inherits,
            RelationshipConfidence::Ambiguous,
        )
    }
}

fn query_specs(language: Language) -> &'static [QuerySpec] {
    match language {
        Language::Cpp => CPP_QUERIES,
        Language::Rust => RUST_QUERIES,
        Language::Python => PYTHON_QUERIES,
        Language::TypeScript | Language::JavaScript => TYPESCRIPT_QUERIES,
        Language::CSharp => CSHARP_QUERIES,
        Language::Go => GO_QUERIES,
    }
}

const CPP_QUERIES: &[QuerySpec] = &[
    QuerySpec {
        kind: RelationshipKind::Calls,
        source: "(call_expression function: (_) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Imports,
        source: "(preproc_include path: (_) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Inherits,
        source: r#"
          (class_specifier name: (type_identifier) @source
            (base_class_clause
              [(type_identifier) (qualified_identifier) (template_type)] @target)) @site
          (struct_specifier name: (type_identifier) @source
            (base_class_clause
              [(type_identifier) (qualified_identifier) (template_type)] @target)) @site
        "#,
        csharp_base_classification: false,
    },
];

const RUST_QUERIES: &[QuerySpec] = &[
    QuerySpec {
        kind: RelationshipKind::Calls,
        source: "(call_expression function: (_) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Imports,
        source: "(use_declaration argument: (_) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Inherits,
        source: r#"
          (trait_item name: (type_identifier) @source
            (trait_bounds
              [(type_identifier) (scoped_type_identifier) (generic_type)] @target)) @site
        "#,
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Implements,
        source: "(impl_item trait: (_) @target type: (_) @source) @site",
        csharp_base_classification: false,
    },
];

const PYTHON_QUERIES: &[QuerySpec] = &[
    QuerySpec {
        kind: RelationshipKind::Calls,
        source: "(call function: (_) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Imports,
        source: r#"
          (import_statement name: (_) @target) @site
          (import_from_statement module_name: (_) @target) @site
        "#,
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Inherits,
        source: r#"
          (class_definition name: (identifier) @source
            superclasses: (argument_list (_) @target)) @site
        "#,
        csharp_base_classification: false,
    },
];

const TYPESCRIPT_QUERIES: &[QuerySpec] = &[
    QuerySpec {
        kind: RelationshipKind::Calls,
        source: r#"
          (call_expression function: (_) @target) @site
          (new_expression constructor: (_) @target) @site
        "#,
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Imports,
        source: "(import_statement source: (string) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Inherits,
        source: r#"
          (class_declaration name: (type_identifier) @source
            (class_heritage (extends_clause value: (_) @target))) @site
          (interface_declaration name: (type_identifier) @source
            (extends_type_clause (_) @target)) @site
        "#,
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Implements,
        source: r#"
          (class_declaration name: (type_identifier) @source
            (class_heritage (implements_clause (_) @target))) @site
        "#,
        csharp_base_classification: false,
    },
];

const CSHARP_QUERIES: &[QuerySpec] = &[
    QuerySpec {
        kind: RelationshipKind::Calls,
        source: r#"
          (invocation_expression function: (_) @target) @site
          (object_creation_expression type: (_) @target) @site
        "#,
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Imports,
        source: r#"
          (using_directive [(identifier) (qualified_name)] @target) @site
        "#,
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Inherits,
        source: r#"
          (class_declaration name: (identifier) @source
            (base_list [(identifier) (generic_name) (qualified_name)] @target)) @site
          (struct_declaration name: (identifier) @source
            (base_list [(identifier) (generic_name) (qualified_name)] @target)) @site
          (interface_declaration name: (identifier) @source
            (base_list [(identifier) (generic_name) (qualified_name)] @target)) @site
        "#,
        csharp_base_classification: true,
    },
];

const GO_QUERIES: &[QuerySpec] = &[
    QuerySpec {
        kind: RelationshipKind::Calls,
        source: "(call_expression function: (_) @target) @site",
        csharp_base_classification: false,
    },
    QuerySpec {
        kind: RelationshipKind::Imports,
        source: "(import_spec path: (interpreted_string_literal) @target) @site",
        csharp_base_classification: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::SymbolParser;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn all_language_queries_compile_and_match_fixture_relationships() {
        let fixtures = [
            (
                Language::Cpp,
                "service.hpp",
                "#include <worker.hpp>\nstruct Base {}; struct Worker : Base { void run() { dispatch(); } };",
                &[RelationshipKind::Imports, RelationshipKind::Inherits, RelationshipKind::Calls][..],
            ),
            (
                Language::Rust,
                "service.rs",
                "use crate::Base; trait Child: Base {} struct Worker; impl Base for Worker { fn run(&self) { dispatch(); } }",
                &[
                    RelationshipKind::Imports,
                    RelationshipKind::Inherits,
                    RelationshipKind::Implements,
                    RelationshipKind::Calls,
                ][..],
            ),
            (
                Language::Python,
                "service.py",
                "from worker import Worker\nclass Child(Worker):\n    def run(self):\n        dispatch()\n",
                &[RelationshipKind::Imports, RelationshipKind::Inherits, RelationshipKind::Calls][..],
            ),
            (
                Language::TypeScript,
                "service.ts",
                "import { Base } from './base'; interface Service {} class Worker extends Base implements Service { run() { dispatch(); } }",
                &[
                    RelationshipKind::Imports,
                    RelationshipKind::Inherits,
                    RelationshipKind::Implements,
                    RelationshipKind::Calls,
                ][..],
            ),
            (
                Language::JavaScript,
                "service.js",
                "import Base from './base'; class Worker extends Base { run() { dispatch(); } }",
                &[RelationshipKind::Imports, RelationshipKind::Inherits, RelationshipKind::Calls][..],
            ),
            (
                Language::CSharp,
                "Service.cs",
                "using App.Core; interface IService {} class Base {} class Worker : Base, IService { void Run() { Dispatch(); } }",
                &[
                    RelationshipKind::Imports,
                    RelationshipKind::Inherits,
                    RelationshipKind::Implements,
                    RelationshipKind::Calls,
                ][..],
            ),
            (
                Language::Go,
                "service.go",
                "package service\nimport \"example/worker\"\nfunc Dispatch() { worker.Run() }\n",
                &[RelationshipKind::Imports, RelationshipKind::Calls][..],
            ),
        ];

        for (language, filename, source, expected) in fixtures {
            let mut parser = SymbolParser::new();
            let parsed = parser
                .parse_source_with_relationships(source, language, Path::new(filename))
                .unwrap_or_else(|error| {
                    let mut diagnostic_parser = tree_sitter::Parser::new();
                    diagnostic_parser
                        .set_language(&language.ts_language())
                        .unwrap();
                    let tree = diagnostic_parser.parse(source, None).unwrap();
                    panic!(
                        "{language} queries failed: {error:#}\n{}",
                        tree.root_node().to_sexp()
                    )
                });
            let found: BTreeSet<_> = parsed
                .relationships
                .iter()
                .map(|relationship| relationship.kind)
                .collect();
            assert!(
                parsed.relationships.iter().all(|relationship| {
                    relationship.start_line > 0
                        && relationship.end_line >= relationship.start_line
                        && relationship.end_byte > relationship.start_byte
                }),
                "{language} emitted an invalid source span: {:#?}",
                parsed.relationships
            );
            for kind in expected {
                assert!(
                    found.contains(kind),
                    "{language} did not extract {kind}; got {:#?}",
                    parsed.relationships
                );
            }
        }
    }

    #[test]
    fn coverage_records_language_semantics_without_fabricating_go_supertypes() {
        for language in [
            Language::Cpp,
            Language::Rust,
            Language::Python,
            Language::TypeScript,
            Language::JavaScript,
            Language::CSharp,
            Language::Go,
        ] {
            let coverage = coverage(language);
            assert!(coverage.calls, "{language}");
            assert!(coverage.imports, "{language}");
        }
        assert!(!coverage(Language::Go).inherits);
        assert!(!coverage(Language::Go).implements);
        assert!(!coverage(Language::Cpp).implements);
        assert!(!coverage(Language::Python).implements);
    }

    #[test]
    fn calls_are_attributed_to_nearest_owning_symbol() {
        let source = "fn outer() { helper(); }";
        let mut parser = SymbolParser::new();
        let parsed = parser
            .parse_source_with_relationships(source, Language::Rust, Path::new("owner.rs"))
            .unwrap();
        let call = parsed
            .relationships
            .iter()
            .find(|relationship| relationship.kind == RelationshipKind::Calls)
            .unwrap();
        assert_eq!(call.source_symbol.as_deref(), Some("outer"));
        assert_eq!(call.target, "helper");
        assert_eq!(call.confidence, RelationshipConfidence::Extracted);
    }
}
