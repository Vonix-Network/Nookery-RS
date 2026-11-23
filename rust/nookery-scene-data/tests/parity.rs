//! Value-level parity gate for the generated scene tables.
//!
//! This is a **second, independent reader** of the C sources: it does not call
//! the generator, does not import its parser, and re-derives every expected
//! value from the raw `.c` bytes plus the repository headers. It then compares
//! those values element-by-element against the integers actually present in the
//! generated Rust text.
//!
//! An earlier version of this file only compared *counts* and therefore passed
//! while a generated value was tampered with. That is exactly the failure mode a
//! parity gate exists to catch, so every assertion here is about values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives under rust/")
        .parent()
        .expect("rust/ lives under the repo root")
        .to_path_buf()
}

fn strip_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '*' {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
            out.push(' ');
        } else if chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '/' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

/// Collect the brace-balanced body of an initializer starting at `lines[index]`.
///
/// Returns the body text and the index of the line after the closing `};`.
/// A naive "stop at the first line beginning with `}`" reader truncates every
/// multi-row table at its first element, which silently compares too few rows.
fn collect_body(lines: &[&str], mut index: usize) -> (String, usize) {
    let mut body = String::new();
    let mut depth = 0i32;
    while index < lines.len() {
        let line = lines[index];
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        let trimmed = line.trim();
        // The whole initializer ends with `};`. A nested row closes with `},`
        // at depth 0, so testing `depth <= 0` here would truncate any
        // multi-row table at its first element.
        if depth < 0 || trimmed.contains("};") {
            return (body, index + 1);
        }
        body.push_str(line);
        body.push(' ');
        index += 1;
    }
    (body, index)
}

/// Evaluate a positional C enum straight out of a repository header.
fn enum_from_header(root: &Path, header: &str, opening: &str) -> BTreeMap<String, i64> {
    let text = std::fs::read_to_string(root.join(header)).expect("header must exist");
    let start = text
        .find(opening)
        .unwrap_or_else(|| panic!("{opening} not found in {header}"));
    let open = text[start..].find('{').expect("enum body") + start;
    let close = text[open..].find("};").expect("enum end") + open;
    let mut values = BTreeMap::new();
    let mut next = 0i64;
    for raw in text[open + 1..close].split(',') {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (name, value) = match line.split_once('=') {
            Some((name, value)) => (
                name.trim().to_string(),
                value
                    .trim()
                    .parse::<i64>()
                    .expect("enum value is an integer"),
            ),
            None => (line.to_string(), next),
        };
        if !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            values.insert(name.clone(), value);
            next = value + 1;
        }
    }
    assert!(!values.is_empty(), "no enumerators parsed from {opening}");
    values
}

/// Resolve a C initializer token to its integer value.
fn resolve(token: &str, symbols: &BTreeMap<String, i64>) -> Option<i64> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    if let Some(value) = symbols.get(token) {
        return Some(*value);
    }
    let cleaned = token.replace(['(', ')'], "");
    let cleaned = cleaned.trim();
    if let Ok(value) = cleaned.parse::<i64>() {
        return Some(value);
    }
    if let Some(hex) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).ok();
    }
    None
}

/// Split on commas at brace/paren depth zero.
fn split_top_level(body: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut brace = 0i32;
    let mut paren = 0i32;
    let mut current = String::new();
    for ch in body.chars() {
        match ch {
            '{' => brace += 1,
            '}' => brace -= 1,
            '(' => paren += 1,
            ')' => paren -= 1,
            _ => {}
        }
        if ch == ',' && brace == 0 && paren == 0 {
            items.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        items.push(current.trim().to_string());
    }
    items.into_iter().filter(|item| !item.is_empty()).collect()
}

/// Extract the integers of one generated `pub const NAME: &[i16] = &[ ... ];`.
fn generated_slice(generated: &str, ident: &str, ty: &str) -> Option<Vec<i64>> {
    let marker = format!("pub const {ident}: &[{ty}] = &[");
    let start = generated.find(&marker)?;
    let body = &generated[start + marker.len()..];
    let end = body.find("];")?;
    Some(
        body[..end]
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|token| !token.is_empty())
            .filter_map(|token| token.parse::<i64>().ok())
            .collect(),
    )
}

#[test]
fn s16_declaration_tables_match_generated_values() {
    let root = repo_root();
    let generated =
        std::fs::read_to_string(root.join("rust/nookery-scene-data/src/generated/scene_words.rs"))
            .expect("generated scene_words.rs");

    let mut symbols = enum_from_header(&root, "include/m_actor.h", "enum actor_profile_table {");
    symbols.extend(enum_from_header(
        &root,
        "include/m_actor.h",
        "enum bank_id {",
    ));

    let mut tables = 0usize;
    let mut values_compared = 0usize;
    let mut problems: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(root.join("src/data/scene")).expect("scene dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().map(|ext| ext != "c").unwrap_or(true) {
            continue;
        }
        let text = strip_comments(&std::fs::read_to_string(&path).expect("read source"));
        let lines: Vec<&str> = text.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index].trim();
            let is_table = (line.starts_with("extern s16 ") || line.starts_with("s16 "))
                && line.contains("[]")
                && line.contains('=');
            if !is_table {
                index += 1;
                continue;
            }
            let symbol = line
                .trim_start_matches("extern ")
                .trim_start_matches("s16 ")
                .split('[')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            index += 1;
            let (body, next_index) = collect_body(&lines, index);
            index = next_index;

            let expected: Vec<i64> = split_top_level(&body)
                .iter()
                .map(|token| {
                    resolve(token, &symbols).unwrap_or_else(|| {
                        panic!(
                            "{}: cannot resolve s16 entry {token:?} in {symbol}",
                            path.display()
                        )
                    })
                })
                .collect();

            let ident = symbol.to_uppercase();
            match generated_slice(&generated, &ident, "i16") {
                None => problems.push(format!(
                    "{}: no generated &[i16] const for {symbol}",
                    path.display()
                )),
                Some(actual) => {
                    if actual != expected {
                        let first = actual
                            .iter()
                            .zip(&expected)
                            .position(|(a, b)| a != b)
                            .unwrap_or_else(|| actual.len().min(expected.len()));
                        problems.push(format!(
                            "{}: {symbol} differs at index {first}: generated={:?} c={:?}",
                            path.display(),
                            actual.get(first),
                            expected.get(first)
                        ));
                    } else {
                        tables += 1;
                        values_compared += expected.len();
                    }
                }
            }
        }
    }

    assert!(tables > 0, "no s16 tables were compared");
    assert!(values_compared > 0, "s16 tables compared zero values");
    assert!(
        problems.is_empty(),
        "{} value mismatches: {problems:#?}",
        problems.len()
    );
    println!("s16_tables={tables} values={values_compared}");
}

#[test]
fn actor_tables_match_generated_values() {
    let root = repo_root();
    let generated =
        std::fs::read_to_string(root.join("rust/nookery-scene-data/src/generated/scene_words.rs"))
            .expect("generated scene_words.rs");
    let symbols = enum_from_header(&root, "include/m_actor.h", "enum actor_profile_table {");

    let mut tables = 0usize;
    let mut rows = 0usize;
    let mut problems: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(root.join("src/data/scene")).expect("scene dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().map(|ext| ext != "c").unwrap_or(true) {
            continue;
        }
        let text = strip_comments(&std::fs::read_to_string(&path).expect("read source"));
        let lines: Vec<&str> = text.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index].trim();
            let is_table = (line.starts_with("extern Actor_data ")
                || line.starts_with("Actor_data "))
                && line.contains("[]")
                && line.contains('=');
            if !is_table {
                index += 1;
                continue;
            }
            let symbol = line
                .trim_start_matches("extern ")
                .trim_start_matches("Actor_data ")
                .split('[')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            index += 1;
            let (body, next_index) = collect_body(&lines, index);
            index = next_index;

            let mut expected: Vec<i64> = Vec::new();
            for row in split_top_level(&body) {
                let inner = row.trim().trim_start_matches('{').trim_end_matches('}');
                let fields = split_top_level(inner);
                assert_eq!(fields.len(), 4, "{}: {symbol} row arity", path.display());
                expected.push(
                    resolve(&fields[0], &symbols)
                        .unwrap_or_else(|| panic!("unresolved profile {}", fields[0])),
                );
                for vector in &fields[1..3] {
                    for component in
                        split_top_level(vector.trim().trim_start_matches('{').trim_end_matches('}'))
                    {
                        expected.push(
                            resolve(&component, &symbols)
                                .unwrap_or_else(|| panic!("unresolved component {component}")),
                        );
                    }
                }
                expected.push(
                    resolve(&fields[3], &symbols)
                        .unwrap_or_else(|| panic!("unresolved arg {}", fields[3])),
                );
                rows += 1;
            }

            let ident = symbol.to_uppercase();
            // Actor consts carry extra syntax; pull the integers in source order.
            let marker = format!("pub const {ident}: &[ActorData] = &[");
            match generated.find(&marker) {
                None => problems.push(format!(
                    "{}: no generated &[ActorData] const for {symbol}",
                    path.display()
                )),
                Some(start) => {
                    let slice = &generated[start + marker.len()..];
                    let end = slice.find("];").expect("actor const terminated");
                    let actual: Vec<i64> = slice[..end]
                        .split(|c: char| {
                            c == ','
                                || c == ' '
                                || c == '['
                                || c == ']'
                                || c == '\n'
                                || c == '{'
                                || c == '}'
                        })
                        .filter(|token| !token.is_empty())
                        .filter_map(|token| {
                            token
                                .strip_prefix("profile:")
                                .or_else(|| token.strip_prefix("arg:"))
                                .unwrap_or(token)
                                .parse::<i64>()
                                .ok()
                        })
                        .collect();
                    if actual != expected {
                        let first = actual
                            .iter()
                            .zip(&expected)
                            .position(|(a, b)| a != b)
                            .unwrap_or_else(|| actual.len().min(expected.len()));
                        problems.push(format!(
                            "{}: {symbol} differs at index {first}: generated={:?} c={:?}",
                            path.display(),
                            actual.get(first),
                            expected.get(first)
                        ));
                    } else {
                        tables += 1;
                    }
                }
            }
        }
    }

    assert!(tables > 0, "no Actor_data tables were compared");
    assert!(rows > 0, "no actor rows were compared");
    assert!(
        problems.is_empty(),
        "{} mismatches: {problems:#?}",
        problems.len()
    );
    println!("actor_tables={tables} rows={rows}");
}

#[test]
fn every_scene_source_has_a_generated_module() {
    let root = repo_root();
    let generated =
        std::fs::read_to_string(root.join("rust/nookery-scene-data/src/generated/scene_words.rs"))
            .expect("generated scene_words.rs");

    let mut sources: Vec<String> = std::fs::read_dir(root.join("src/data/scene"))
        .expect("scene dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "c"))
        .map(|entry| {
            entry
                .path()
                .file_stem()
                .expect("stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    sources.sort();
    assert!(!sources.is_empty(), "no scene sources found");

    let missing: Vec<&String> = sources
        .iter()
        .filter(|module| !generated.contains(&format!("pub mod {module} {{")))
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {} sources have no generated module: {missing:?}",
        missing.len(),
        sources.len()
    );
    println!("modules={}", sources.len());
}
