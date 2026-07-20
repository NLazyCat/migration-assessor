use std::collections::{HashMap, HashSet};

use super::types::LibraryEntry;

pub(crate) fn find_best_match(
    src: &LibraryEntry,
    target_registry: &HashMap<String, LibraryEntry>,
) -> Option<(String, f64)> {
    target_registry
        .iter()
        .map(|(name, tgt)| (name.clone(), compute_similarity(src, tgt)))
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .filter(|(_, score)| *score > 0.0)
}

pub(crate) fn compute_similarity(src: &LibraryEntry, tgt: &LibraryEntry) -> f64 {
    let tag_sim = jaccard_similarity(&src.tags, &tgt.tags);
    let type_bonus = if src.lib_type == tgt.lib_type {
        1.0
    } else {
        0.0
    };
    tag_sim * 0.8 + type_bonus * 0.2
}

pub(crate) fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    let a_set: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let b_set: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = a_set.intersection(&b_set).count();
    let union = a_set.union(&b_set).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub(crate) fn score_to_compatibility(score: f64) -> super::types::CompatibilityLevel {
    if score >= 0.5 {
        super::types::CompatibilityLevel::Full
    } else if score >= 0.25 {
        super::types::CompatibilityLevel::Partial
    } else if score > 0.0 {
        super::types::CompatibilityLevel::None
    } else {
        super::types::CompatibilityLevel::Unknown
    }
}

#[cfg(test)]
mod tests {
use super::*;
use crate::compatibility::CompatibilityLevel;

fn entry(tags: &[&str], lib_type: &str) -> LibraryEntry {
        LibraryEntry {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            lib_type: lib_type.to_string(),
            description: String::new(),
        }
    }

    #[test]
    fn test_jaccard_identical() {
        let a = vec!["http".to_string(), "server".to_string()];
        let b = vec!["http".to_string(), "server".to_string()];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let a = vec!["http".to_string()];
        let b = vec!["db".to_string()];
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_partial() {
        let a = vec!["http".to_string(), "server".to_string()];
        let b = vec!["http".to_string(), "client".to_string()];
        assert!((jaccard_similarity(&a, &b) - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_empty() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec!["a".to_string()];
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_both_empty() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_similarity_same_type() {
        let a = entry(&["http", "server"], "framework");
        let b = entry(&["http", "server"], "framework");
        let sim = compute_similarity(&a, &b);
        // tag sim = 1.0 * 0.8 + type bonus = 1.0 * 0.2 = 1.0
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_similarity_different_type() {
        let a = entry(&["http"], "framework");
        let b = entry(&["http"], "library");
        let sim = compute_similarity(&a, &b);
        // tag sim = 1.0 * 0.8 + type bonus = 0.0 * 0.2 = 0.8
        assert!((sim - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_find_best_match() {
        let src = entry(&["http", "router"], "framework");
        let mut targets = HashMap::new();
        targets.insert("express".to_string(), entry(&["http", "router"], "framework"));
        targets.insert("lodash".to_string(), entry(&["utility"], "library"));
        let best = find_best_match(&src, &targets);
        assert!(best.is_some());
        let (name, _score) = best.unwrap();
        assert_eq!(name, "express");
    }

    #[test]
    fn test_find_best_match_no_match() {
        let src = entry(&["unique"], "unknown");
        let mut targets = HashMap::new();
        targets.insert("other".to_string(), entry(&["completely", "different"], "other"));
        let best = find_best_match(&src, &targets);
        assert!(best.is_none());
    }

    #[test]
    fn test_score_to_compatibility() {
        assert_eq!(score_to_compatibility(0.6), CompatibilityLevel::Full);
        assert_eq!(score_to_compatibility(0.3), CompatibilityLevel::Partial);
        assert_eq!(score_to_compatibility(0.1), CompatibilityLevel::None);
        assert_eq!(score_to_compatibility(0.0), CompatibilityLevel::Unknown);
    }
}