use crate::shadow::index::{EntryKind, IndexEntry};
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};

const TS_QUERY: &str = r#"
(export_statement
  declaration: [
    (function_declaration name: (identifier) @name)
    (class_declaration name: (type_identifier) @name)
    (interface_declaration name: (type_identifier) @name)
    (type_alias_declaration name: (type_identifier) @name)
    (enum_declaration name: (identifier) @name)
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
  ]) @export

(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name))) @reexport
"#;

const PY_QUERY: &str = r#"
(function_definition name: (identifier) @name) @fn
(decorated_definition
  (function_definition name: (identifier) @name)) @fn
(class_definition name: (identifier) @name) @cls
(expression_statement
  (assignment
    left: (identifier) @name)) @const
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    TypeScript,
    Tsx,
    Python,
}

pub fn lang_for_path(path: &Path) -> Option<Lang> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Some(Lang::TypeScript),
        "tsx" | "jsx" => Some(Lang::Tsx),
        "py" | "pyi" => Some(Lang::Python),
        _ => None,
    }
}

pub struct SymbolExtractor {
    ts_parser: Parser,
    tsx_parser: Parser,
    py_parser: Parser,
    ts_query: Query,
    tsx_query: Query,
    py_query: Query,
    ts_lang: Language,
    tsx_lang: Language,
    py_lang: Language,
}

impl SymbolExtractor {
    pub fn new() -> anyhow::Result<Self> {
        let ts_lang = Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT);
        let tsx_lang = Language::new(tree_sitter_typescript::LANGUAGE_TSX);
        let py_lang = Language::new(tree_sitter_python::LANGUAGE);

        let ts_query = Query::new(&ts_lang, TS_QUERY)?;
        let tsx_query = Query::new(&tsx_lang, TS_QUERY)?;
        let py_query = Query::new(&py_lang, PY_QUERY)?;

        let mut ts_parser = Parser::new();
        ts_parser.set_language(&ts_lang)?;
        let mut tsx_parser = Parser::new();
        tsx_parser.set_language(&tsx_lang)?;
        let mut py_parser = Parser::new();
        py_parser.set_language(&py_lang)?;

        Ok(Self {
            ts_parser,
            tsx_parser,
            py_parser,
            ts_query,
            tsx_query,
            py_query,
            ts_lang,
            tsx_lang,
            py_lang,
        })
    }

    pub fn extract(&mut self, lang: Lang, source: &[u8], rel_path: &str) -> Vec<IndexEntry> {
        match lang {
            Lang::TypeScript => self.extract_ts(source, rel_path, false),
            Lang::Tsx => self.extract_ts(source, rel_path, true),
            Lang::Python => self.extract_py(source, rel_path),
        }
    }

    fn extract_ts(&mut self, source: &[u8], rel_path: &str, tsx: bool) -> Vec<IndexEntry> {
        let (parser, lang) = if tsx {
            (&mut self.tsx_parser, &self.tsx_lang)
        } else {
            (&mut self.ts_parser, &self.ts_lang)
        };
        let _ = parser.set_language(lang);
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let query = if tsx { &self.tsx_query } else { &self.ts_query };
        let name_idx = match query.capture_index_for_name("name") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source);

        let mut entries = Vec::new();
        while let Some(m) = matches.next() {
            for cap in m.captures {
                if cap.index != name_idx {
                    continue;
                }
                let node = cap.node;
                let name = node_text(node, source);
                if name.is_empty() {
                    continue;
                }
                let kind = kind_from_ts_node(node);
                let location = format!("{}:{}", rel_path, node.start_position().row + 1);
                entries.push(IndexEntry {
                    name,
                    kind,
                    location,
                });
            }
        }
        entries
    }

    fn extract_py(&mut self, source: &[u8], rel_path: &str) -> Vec<IndexEntry> {
        let _ = self.py_parser.set_language(&self.py_lang);
        let tree = match self.py_parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let name_idx = match self.py_query.capture_index_for_name("name") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.py_query, tree.root_node(), source);

        let mut entries = Vec::new();
        while let Some(m) = matches.next() {
            for cap in m.captures {
                if cap.index != name_idx {
                    continue;
                }
                let node = cap.node;
                let name = node_text(node, source);
                if name.is_empty() || name.starts_with('_') {
                    continue;
                }
                let kind = kind_from_py_pattern(m.pattern_index);
                let location = format!("{}:{}", rel_path, node.start_position().row + 1);
                entries.push(IndexEntry {
                    name,
                    kind,
                    location,
                });
            }
        }
        entries
    }
}

fn node_text(node: Node<'_>, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}

fn kind_from_ts_node(name_node: Node<'_>) -> EntryKind {
    let mut ancestor = name_node.parent();
    while let Some(node) = ancestor {
        match node.kind() {
            "function_declaration" | "function" => return EntryKind::Function,
            "class_declaration" => return EntryKind::Class,
            "interface_declaration" => return EntryKind::Interface,
            "type_alias_declaration" => return EntryKind::Type,
            "enum_declaration" => return EntryKind::Type,
            "lexical_declaration" | "variable_declarator" => return EntryKind::Const,
            "export_clause" | "export_specifier" => return EntryKind::Export,
            _ => {}
        }
        ancestor = node.parent();
    }
    EntryKind::Export
}

fn kind_from_py_pattern(pattern_idx: usize) -> EntryKind {
    match pattern_idx {
        0 | 1 => EntryKind::Function,
        2 => EntryKind::Class,
        _ => EntryKind::Const,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn lang_for_path_typescript_extensions() {
        for ext in &["ts", "mts", "cts", "js", "mjs", "cjs"] {
            let name = format!("file.{ext}");
            assert_eq!(lang_for_path(Path::new(&name)), Some(Lang::TypeScript), "ext={ext}");
        }
    }

    #[test]
    fn lang_for_path_tsx_extensions() {
        for ext in &["tsx", "jsx"] {
            let name = format!("file.{ext}");
            assert_eq!(lang_for_path(Path::new(&name)), Some(Lang::Tsx), "ext={ext}");
        }
    }

    #[test]
    fn lang_for_path_python_extensions() {
        for ext in &["py", "pyi"] {
            let name = format!("file.{ext}");
            assert_eq!(lang_for_path(Path::new(&name)), Some(Lang::Python), "ext={ext}");
        }
    }

    #[test]
    fn lang_for_path_unknown_extensions() {
        for ext in &["rs", "go", "c", "cpp", "md", "json"] {
            let name = format!("file.{ext}");
            assert_eq!(lang_for_path(Path::new(&name)), None, "ext={ext}");
        }
    }

    #[test]
    fn extract_typescript_exported_function() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"export function greet(name: string): string { return name; }";
        let entries = ext.extract(Lang::TypeScript, src, "src/greet.ts");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "greet");
        assert_eq!(entries[0].kind, EntryKind::Function);
        assert!(entries[0].location.starts_with("src/greet.ts:"));
    }

    #[test]
    fn extract_typescript_exported_class() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"export class Greeter { greet() {} }";
        let entries = ext.extract(Lang::TypeScript, src, "src/greeter.ts");
        assert!(entries.iter().any(|e| e.name == "Greeter" && e.kind == EntryKind::Class));
    }

    #[test]
    fn extract_typescript_exported_interface() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"export interface User { name: string; }";
        let entries = ext.extract(Lang::TypeScript, src, "src/user.ts");
        assert!(entries.iter().any(|e| e.name == "User" && e.kind == EntryKind::Interface));
    }

    #[test]
    fn extract_typescript_no_exports_returns_empty() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"function internal() {}";
        let entries = ext.extract(Lang::TypeScript, src, "src/internal.ts");
        assert!(entries.is_empty());
    }

    #[test]
    fn extract_python_function() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"def compute(x, y):\n    return x + y\n";
        let entries = ext.extract(Lang::Python, src, "src/math.py");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "compute");
        assert_eq!(entries[0].kind, EntryKind::Function);
    }

    #[test]
    fn extract_python_class() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"class Processor:\n    pass\n";
        let entries = ext.extract(Lang::Python, src, "src/proc.py");
        assert!(entries.iter().any(|e| e.name == "Processor" && e.kind == EntryKind::Class));
    }

    #[test]
    fn extract_python_skips_private_names() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"def _private():\n    pass\ndef public():\n    pass\n";
        let entries = ext.extract(Lang::Python, src, "src/mod.py");
        assert!(entries.iter().all(|e| e.name != "_private"));
        assert!(entries.iter().any(|e| e.name == "public"));
    }

    #[test]
    fn extract_from_bytes_skips_unsupported_extension() {
        let mut ext = SymbolExtractor::new().unwrap();
        let src = b"fn main() {}";
        let entries = ext.extract_from_bytes(
            Path::new("src/main.rs"),
            src,
            "src/main.rs",
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn extract_from_bytes_skips_oversized_files() {
        let mut ext = SymbolExtractor::new().unwrap();
        let big: Vec<u8> = b"export function f() {} ".iter().cycle().take(201 * 1024).copied().collect();
        let entries = ext.extract_from_bytes(Path::new("src/big.ts"), &big, "src/big.ts");
        assert!(entries.is_empty());
    }
}
