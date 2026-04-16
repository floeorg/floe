//! Reference tracking for LSP features.
//!
//! `ReferenceTracker` builds a side-table of `(definition_span, reference_span)`
//! pairs during type checking. Once the checker has finished, LSP features
//! (go-to-definition, find-references, rename) consult the tracker by span
//! instead of re-walking the AST for every query.
//!
//! The tracker is keyed by `Span` (byte offsets into the source) so lookup
//! is O(1) and survives the type-checked tree: code that holds a definition's
//! `Span` can later ask for every reference without touching the AST again.
//!
//! Designed to live inside `ModuleInterface` once #1111 lands — until then
//! it's attached to the checker's output and consumed by the LSP via the
//! `Checker::references()` accessor.
//!
//! Mirrors Gleam's `reference.rs`.
use std::collections::HashMap;

use crate::lexer::span::Span;

#[derive(Debug, Clone, Default)]
pub struct ReferenceTracker {
    /// Every reference span, keyed by the definition span it points to.
    refs_by_definition: HashMap<Span, Vec<Span>>,
    /// Reverse index: every reference span maps to its definition.
    definition_by_reference: HashMap<Span, Span>,
    /// All definition spans that have been registered, even if no references
    /// exist yet. Lets `definition_for_name` / iteration pick up unused defs.
    definitions: HashMap<String, Span>,
}

impl ReferenceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a definition site. Called once for each named entity
    /// (function, const, type, import) at the point the checker enters it.
    pub fn register_definition(&mut self, name: &str, definition_span: Span) {
        self.definitions.insert(name.to_string(), definition_span);
        self.refs_by_definition.entry(definition_span).or_default();
    }

    /// Record that `reference_span` refers to the symbol defined at
    /// `definition_span`. Idempotent: the same pair can be recorded
    /// repeatedly without growing the list (useful when the checker
    /// visits the same identifier through multiple passes).
    pub fn record(&mut self, definition_span: Span, reference_span: Span) {
        let refs = self.refs_by_definition.entry(definition_span).or_default();
        if !refs.contains(&reference_span) {
            refs.push(reference_span);
        }
        self.definition_by_reference
            .insert(reference_span, definition_span);
    }

    /// Every reference to the symbol defined at `definition_span`. The
    /// definition itself is not included — callers that want "definition
    /// plus all uses" can prepend it.
    pub fn find_references(&self, definition_span: Span) -> Vec<Span> {
        self.refs_by_definition
            .get(&definition_span)
            .cloned()
            .unwrap_or_default()
    }

    /// The definition span a reference at `reference_span` points to,
    /// when one is known. Used by go-to-definition.
    pub fn definition_at(&self, reference_span: Span) -> Option<Span> {
        self.definition_by_reference.get(&reference_span).copied()
    }

    /// Definition span for a name registered via `register_definition`.
    /// Convenience for callers that only have the name (e.g. rename from
    /// a symbol table key) and need its span to query references.
    pub fn definition_for_name(&self, name: &str) -> Option<Span> {
        self.definitions.get(name).copied()
    }

    /// All `(definition_span, references)` pairs currently tracked.
    pub fn entries(&self) -> impl Iterator<Item = (Span, &Vec<Span>)> {
        self.refs_by_definition.iter().map(|(k, v)| (*k, v))
    }

    /// Number of distinct definitions tracked.
    pub fn definition_count(&self) -> usize {
        self.refs_by_definition.len()
    }

    /// Number of recorded references across all definitions.
    pub fn reference_count(&self) -> usize {
        self.refs_by_definition.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(start: usize, end: usize) -> Span {
        Span::new(start, end, 1, start + 1)
    }

    #[test]
    fn find_references_returns_every_recorded_use() {
        let mut t = ReferenceTracker::new();
        let def = sp(0, 5);
        t.register_definition("foo", def);
        t.record(def, sp(10, 13));
        t.record(def, sp(20, 23));
        t.record(def, sp(30, 33));

        let refs = t.find_references(def);
        assert_eq!(refs.len(), 3);
        assert!(refs.contains(&sp(10, 13)));
        assert!(refs.contains(&sp(20, 23)));
        assert!(refs.contains(&sp(30, 33)));
    }

    #[test]
    fn definition_at_gives_back_the_definition_span() {
        let mut t = ReferenceTracker::new();
        let def = sp(0, 5);
        let use_site = sp(10, 13);
        t.register_definition("foo", def);
        t.record(def, use_site);
        assert_eq!(t.definition_at(use_site), Some(def));
    }

    #[test]
    fn recording_the_same_reference_twice_is_idempotent() {
        let mut t = ReferenceTracker::new();
        let def = sp(0, 5);
        let use_site = sp(10, 13);
        t.register_definition("foo", def);
        t.record(def, use_site);
        t.record(def, use_site);
        assert_eq!(t.find_references(def).len(), 1);
    }

    #[test]
    fn register_definition_without_uses_keeps_entry() {
        let mut t = ReferenceTracker::new();
        let def = sp(0, 5);
        t.register_definition("unused_fn", def);
        assert_eq!(t.find_references(def), Vec::<Span>::new());
        assert_eq!(t.definition_for_name("unused_fn"), Some(def));
        assert_eq!(t.definition_count(), 1);
    }

    #[test]
    fn missing_definition_yields_empty_references() {
        let t = ReferenceTracker::new();
        let def = sp(0, 5);
        assert_eq!(t.find_references(def), Vec::<Span>::new());
        assert_eq!(t.definition_at(def), None);
    }

    #[test]
    fn multiple_definitions_keep_their_references_separate() {
        let mut t = ReferenceTracker::new();
        let def_a = sp(0, 3);
        let def_b = sp(10, 13);
        t.register_definition("a", def_a);
        t.register_definition("b", def_b);
        t.record(def_a, sp(20, 23));
        t.record(def_b, sp(30, 33));
        assert_eq!(t.find_references(def_a), vec![sp(20, 23)]);
        assert_eq!(t.find_references(def_b), vec![sp(30, 33)]);
    }
}
