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