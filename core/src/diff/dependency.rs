use crate::diff::{CompatibilityInfo, DependencyChange};
use crate::deps::ResolvedDependency;

pub fn diff_dependencies(old: &[ResolvedDependency], new: &[ResolvedDependency]) -> Vec<DependencyChange> {
    let mut changes = Vec::new();

    let old_by_name: std::collections::HashMap<&str, &ResolvedDependency> = old.iter().map(|d| (d.name.as_str(), d)).collect();
    let new_by_name: std::collections::HashMap<&str, &ResolvedDependency> = new.iter().map(|d| (d.name.as_str(), d)).collect();

    for name in new_by_name.keys().filter(|n| !old_by_name.contains_key(*n)) {
        let new_dep = new_by_name[name];
        changes.push(DependencyChange {
            package: name.to_string(),
            change_type: "added".to_string(),
            old_version: None,
            new_version: Some(new_dep.version.clone()),
            compatibility: CompatibilityInfo {
                equivalent: None,
                compatibility: "unknown".to_string(),
                effort: "unknown".to_string(),
                guidance: None,
                is_high_risk: false,
            },
            affected_modules: Vec::new(),
        });
    }

    for name in old_by_name.keys().filter(|n| !new_by_name.contains_key(*n)) {
        let old_dep = old_by_name[name];
        changes.push(DependencyChange {
            package: name.to_string(),
            change_type: "removed".to_string(),
            old_version: Some(old_dep.version.clone()),
            new_version: None,
            compatibility: CompatibilityInfo {
                equivalent: None,
                compatibility: "removed".to_string(),
                effort: "low".to_string(),
                guidance: None,
                is_high_risk: false,
            },
            affected_modules: Vec::new(),
        });
    }

    for name in old_by_name.keys().filter(|n| new_by_name.contains_key(*n)) {
        let old_dep = old_by_name[name];
        let new_dep = new_by_name[name];

        if old_dep.version != new_dep.version {
            let change_type = compare_versions(&old_dep.version, &new_dep.version);

            changes.push(DependencyChange {
                package: name.to_string(),
                change_type,
                old_version: Some(old_dep.version.clone()),
                new_version: Some(new_dep.version.clone()),
                compatibility: CompatibilityInfo {
                    equivalent: None,
                    compatibility: "unknown".to_string(),
                    effort: "unknown".to_string(),
                    guidance: None,
                    is_high_risk: is_major_bump(&old_dep.version, &new_dep.version),
                },
                affected_modules: Vec::new(),
            });
        }
    }

    changes
}

fn compare_versions(old: &str, new: &str) -> String {
    let old_major = parse_major_version(old);
    let new_major = parse_major_version(new);

    if new_major > old_major {
        "upgraded".to_string()
    } else if new_major < old_major {
        "downgraded".to_string()
    } else {
        let old_minor = parse_minor_version(old);
        let new_minor = parse_minor_version(new);

        if new_minor > old_minor {
            "upgraded".to_string()
        } else if new_minor < old_minor {
            "downgraded".to_string()
        } else {
            "patch".to_string()
        }
    }
}

fn parse_major_version(version: &str) -> u32 {
    version.split('.').next().and_then(|s| s.trim_start_matches('v').parse().ok()).unwrap_or(0)
}

fn parse_minor_version(version: &str) -> u32 {
    version.split('.').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn is_major_bump(old: &str, new: &str) -> bool {
    parse_major_version(new) > parse_major_version(old)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_versions_same() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), "patch");
    }

    #[test]
    fn test_compare_versions_major_upgrade() {
        assert_eq!(compare_versions("1.2.3", "2.0.0"), "upgraded");
    }

    #[test]
    fn test_compare_versions_minor_upgrade() {
        assert_eq!(compare_versions("1.2.3", "1.3.0"), "upgraded");
    }

    #[test]
    fn test_compare_versions_major_downgrade() {
        assert_eq!(compare_versions("2.0.0", "1.2.3"), "downgraded");
    }

    #[test]
    fn test_is_major_bump_true() {
        assert!(is_major_bump("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_is_major_bump_false() {
        assert!(!is_major_bump("1.0.0", "1.1.0"));
    }

    #[test]
    fn test_parse_major_version_with_v_prefix() {
        assert_eq!(parse_major_version("v2.0.0"), 2);
    }

    #[test]
    fn test_version_added_and_removed() {
        let old = vec![ResolvedDependency {
            name: "foo".into(),
            version: "1.0.0".into(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".into(),
        }];
        let new = vec![ResolvedDependency {
            name: "bar".into(),
            version: "2.0.0".into(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".into(),
        }];
        let changes = diff_dependencies(&old, &new);
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|c| c.package == "foo" && c.change_type == "removed"));
        assert!(changes.iter().any(|c| c.package == "bar" && c.change_type == "added"));
    }
}
