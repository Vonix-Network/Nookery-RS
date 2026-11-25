//! Portable, source-grounded representations of the literal model tables.
//!
//! This crate is intentionally independent of the repository workspace and of
//! the C build.  A [`Value::Token`] records a C identifier or macro expression
//! exactly as a data reference; it is not a host pointer.  This preserves the
//! source's ordering and nested initializer shape without making an invalid
//! pointer claim on a Rust host.

/// One source-order initializer value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    /// A signed C integer literal, represented without truncation.
    Integer(i128),
    /// An identifier, reference, NULL, or macro expression from the C source.
    Token(&'static str),
    /// A brace-delimited nested initializer.
    List(&'static [Value]),
}

/// One literal declaration in a model source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    /// C identifier, including the original case and spelling.
    pub name: &'static str,
    /// C declaration type after storage/alignment attributes are removed.
    pub c_type: &'static str,
    /// Number of top-level initializer elements.
    pub entry_count: usize,
    /// SHA-256 of the canonical parsed initializer tree.
    pub values_sha256: &'static str,
    /// The complete source-order initializer tree.
    pub values: &'static [Value],
}

/// One eligible source file and all of its literal declarations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFile {
    /// Repository-relative C path.
    pub source_path: &'static str,
    /// Exact source byte length.
    pub source_bytes: usize,
    /// SHA-256 of the exact source bytes.
    pub source_sha256: &'static str,
    /// Declarations in source order.
    pub declarations: &'static [Declaration],
}

mod generated;

pub use generated::SOURCE_FILES;

/// The checked-in source manifest, useful to external audit tooling.
pub const SOURCE_MANIFEST: &str = include_str!("../SOURCE_MANIFEST.toml");

/// Find one generated source by repository-relative path.
#[must_use]
pub fn source_file(path: &str) -> Option<&'static SourceFile> {
    SOURCE_FILES
        .iter()
        .find(|source| source.source_path == path)
}
