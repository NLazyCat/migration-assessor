use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::deps::ResolvedDependency;

use super::matching::{find_best_match, score_to_compatibility};
use super::types::{
    CompatibilityEntry, CompatibilityLevel, DepChangeInfo, DependencyImpact, GuidanceOverride,
    LanguageRegistry, MigrationEffort, OverrideEntry, derive_risk_tags, parse_effort_str,
    parse_compatibility_str,
};

pub struct CompatibilityMatrix {
    source_language: String,
    target_language: String,
    pub(crate) built_in: HashMap<String, CompatibilityEntry>,
    pub(crate) overrides: HashMap<String, CompatibilityEntry>,
}

impl CompatibilityMatrix {
    pub fn new(source_language: String, target_language: String) -> Self {
        let source_registry: LanguageRegistry = LanguageRegistry::load(&source_language);
        let target_registry: LanguageRegistry = LanguageRegistry::load(&target_language);

        let mut built_in: HashMap<String, CompatibilityEntry> = HashMap::new();

        for (src_name, src_entry) in &source_registry.libraries {
            let best = find_best_match(src_entry, &target_registry.libraries);
            if let Some((tgt_name, score)) = best {
                let compat_level = score_to_compatibility(score);
                let effort = compat_level.default_effort();
                let pct = (score * 100.0) as u32;
                let guidance = Some(format!(
                    "Best match: `{}` (similarity {}%). {}",
                    tgt_name, pct, target_registry.libraries[&tgt_name].description
                ));
                let risk_tags = derive_risk_tags(
                    effort,
                    compat_level,
                    Some(&src_entry.tags),
                    guidance.as_deref(),
                );

                built_in.insert(
                    src_name.clone(),
                    CompatibilityEntry {
                        source_language: source_language.clone(),
                        target_language: target_language.clone(),
                        equivalent: Some(tgt_name.clone()),
                        compatibility: compat_level,
                        effort,
                        guidance,
                        note: None,
                        tags: Some(src_entry.tags.clone()),
                        risk_tags,
                    },
                );
            } else {
                built_in.insert(
                    src_name.clone(),
                    CompatibilityEntry {
                        source_language: source_language.clone(),
                        target_language: target_language.clone(),
                        equivalent: None,
                        compatibility: CompatibilityLevel::Unknown,
                        effort: MigrationEffort::Unknown,
                        guidance: Some("No matching library found in target language.".to_string()),
                        note: Some(format!(
                            "`{}` has no recognizable equivalent in {}.",
                            src_name, target_language
                        )),
                        tags: Some(src_entry.tags.clone()),
                        risk_tags: vec!["unmapped".to_string()],
                    },
                );
            }
        }

        Self {
            source_language,
            target_language,
            built_in,
            overrides: HashMap::new(),
        }
    }

    pub fn source_language(&self) -> &str {
        &self.source_language
    }

    pub fn target_language(&self) -> &str {
        &self.target_language
    }

    pub fn load_overrides(&mut self, path: &Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(path)?;
        let override_file: super::types::CompatibilityOverrideFile = toml::from_str(&content)?;

        for (toml_key, entry) in override_file.dependencies {
            let pkg_name = entry
                .packages
                .as_ref()
                .and_then(|p| p.get(&self.source_language))
                .cloned()
                .unwrap_or_else(|| toml_key.clone());

            let compat_level = entry
                .compatibility
                .as_deref()
                .map(parse_compatibility_str)
                .unwrap_or(CompatibilityLevel::Unknown);

            let effort = entry
                .effort
                .as_deref()
                .map(parse_effort_str)
                .unwrap_or_else(|| compat_level.default_effort());

            let target_pkg = entry
                .packages
                .as_ref()
                .and_then(|p| p.get(&self.target_language))
                .or(entry.equivalent.as_ref())
                .cloned();

            let guidance = match &entry.guidance {
                GuidanceOverride::Single(s) => Some(s.clone()),
                GuidanceOverride::Map(m) => {
                    let dir_key = format!("{}_to_{}", self.source_language, self.target_language);
                    m.get(&dir_key)
                        .or_else(|| {
                            if m.len() == 1 {
                                m.values().next()
                            } else {
                                None
                            }
                        })
                        .cloned()
                }
                GuidanceOverride::None => None,
            };

            let risk_tags = derive_risk_tags(
                effort,
                compat_level,
                entry.tags.as_deref(),
                guidance.as_deref(),
            );

            self.overrides.insert(
                pkg_name,
                CompatibilityEntry {
                    source_language: self.source_language.clone(),
                    target_language: self.target_language.clone(),
                    equivalent: target_pkg,
                    compatibility: compat_level,
                    effort,
                    guidance,
                    note: entry.note,
                    tags: entry.tags,
                    risk_tags,
                },
            );
        }

        Ok(())
    }

    pub fn evaluate(
        &self,
        dependencies: &[ResolvedDependency],
    ) -> HashMap<String, CompatibilityEntry> {
        let mut result = HashMap::new();

        for dep in dependencies {
            if let Some(entry) = self.overrides.get(&dep.name) {
                result.insert(dep.name.clone(), entry.clone());
            } else if let Some(entry) = self.built_in.get(&dep.name) {
                result.insert(dep.name.clone(), entry.clone());
            } else {
                result.insert(
                    dep.name.clone(),
                    CompatibilityEntry {
                        source_language: self.source_language.clone(),
                        target_language: self.target_language.clone(),
                        equivalent: None,
                        compatibility: CompatibilityLevel::Unknown,
                        effort: MigrationEffort::Unknown,
                        guidance: None,
                        note: Some("No compatibility mapping available.".to_string()),
                        tags: None,
                        risk_tags: vec!["unmapped".to_string()],
                    },
                );
            }
        }

        result
    }

    pub fn lookup(&self, package: &str) -> Option<&CompatibilityEntry> {
        self.overrides
            .get(package)
            .or_else(|| self.built_in.get(package))
    }

    pub fn detect_dep_changes(
        &self,
        old_deps: &[ResolvedDependency],
        new_deps: &[ResolvedDependency],
    ) -> Vec<DepChangeInfo> {
        let old_map: HashMap<&str, &ResolvedDependency> =
            old_deps.iter().map(|d| (d.name.as_str(), d)).collect();
        let new_map: HashMap<&str, &ResolvedDependency> =
            new_deps.iter().map(|d| (d.name.as_str(), d)).collect();

        let mut changes = Vec::new();
        let mut all_names: Vec<&str> = old_map.keys().chain(new_map.keys()).copied().collect();
        all_names.sort();
        all_names.dedup();

        for name in all_names {
            let old_dep = old_map.get(name);
            let new_dep = new_map.get(name);

            let change_type = match (old_dep, new_dep) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(old), Some(new)) if old.version != new.version => {
                    if old.version < new.version {
                        "upgraded"
                    } else {
                        "downgraded"
                    }
                }
                _ => continue,
            };

            let old_entry = old_dep.and_then(|d| self.lookup(d.name.as_str()));
            let new_entry = new_dep.and_then(|d| self.lookup(d.name.as_str()));

            let needs_review = match change_type {
                "added" => new_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Unknown
                            || e.compatibility == CompatibilityLevel::None
                    })
                    .unwrap_or(true),
                "removed" => old_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Full
                            || e.compatibility == CompatibilityLevel::Partial
                    })
                    .unwrap_or(false),
                _ => new_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Partial
                            || e.compatibility == CompatibilityLevel::Unknown
                    })
                    .unwrap_or(true),
            };

            let entry = new_entry.or(old_entry);
            changes.push(DepChangeInfo {
                package: name.to_string(),
                old_version: old_dep.map(|d| d.version.clone()),
                new_version: new_dep.map(|d| d.version.clone()),
                change_type: change_type.to_string(),
                compatibility_before: old_entry.map(|e| e.compatibility),
                compatibility_now: new_entry.map(|e| e.compatibility),
                equivalent: new_entry.and_then(|e| e.equivalent.clone()),
                needs_review,
                effort: entry.map(|e| e.effort).unwrap_or(MigrationEffort::Unknown),
                guidance: entry.and_then(|e| e.guidance.clone()),
                risk_tags: entry.map(|e| e.risk_tags.clone()).unwrap_or_default(),
            });
        }

        changes
    }

    pub fn analyze_impact(
        &self,
        dep_changes: &[DepChangeInfo],
        package_modules: &HashMap<String, Vec<String>>,
    ) -> Vec<DependencyImpact> {
        let mut impacts = Vec::new();

        for change in dep_changes {
            let affected_modules = package_modules
                .get(&change.package)
                .cloned()
                .unwrap_or_default();

            let affected_module_count = affected_modules.len();
            let compat = change
                .compatibility_now
                .unwrap_or(CompatibilityLevel::Unknown);
            let is_high_impact = affected_module_count > 5
                && (compat == CompatibilityLevel::Unknown || compat == CompatibilityLevel::None);

            impacts.push(DependencyImpact {
                package: change.package.clone(),
                affected_module_count,
                affected_modules: affected_modules.into_iter().take(20).collect(),
                is_high_impact,
            });
        }

        impacts
    }
}