//! Infer application IDs and default Gradle tasks from `app/build.gradle(.kts)`.
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ProjectInference {
    /// e.g. `ai.wayve.app`, `ai.wayve.app.dev`
    pub application_ids: Vec<String>,
    /// All flavor names (flat, de-duplicated)
    #[allow(dead_code)]
    pub flavor_names: Vec<String>,
    /// Ordered flavor dimensions, e.g. ["track", "environment"]
    #[allow(dead_code)]
    pub flavor_dimensions: Vec<String>,
    /// First flavor selected per dimension: [("track","canary"), ("environment","dev")]
    #[allow(dead_code)]
    pub selected_flavors: Vec<(String, String)>,
    /// Inferred `assemble*` task
    pub assemble_task: Option<String>,
    /// Inferred `install*` task
    pub install_task: Option<String>,
    /// Human-readable summary shown in the sidebar
    pub variant_summary: String,
    pub gradle_file: Option<PathBuf>,
}

pub fn find_app_gradle(project_root: &Path) -> Option<PathBuf> {
    for p in [
        project_root.join("app/build.gradle"),
        project_root.join("app/build.gradle.kts"),
    ] {
        if p.is_file() {
            return Some(p);
        }
    }
    let Ok(entries) = fs::read_dir(project_root) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        for name in ["build.gradle", "build.gradle.kts"] {
            let g = path.join(name);
            if g.is_file() {
                if let Ok(text) = fs::read_to_string(&g) {
                    if text.contains("com.android.application") {
                        return Some(g);
                    }
                }
            }
        }
    }
    None
}

pub fn infer_project(project_root: &Path) -> Result<ProjectInference> {
    let Some(gradle_path) = find_app_gradle(project_root) else {
        return Ok(ProjectInference {
            variant_summary: "no app/build.gradle".to_string(),
            ..Default::default()
        });
    };
    let text = fs::read_to_string(&gradle_path)?;
    let mut inf = infer_from_gradle_text(&text);
    inf.gradle_file = Some(gradle_path);
    Ok(inf)
}

// ─── core parsing ────────────────────────────────────────────────────────────

fn infer_from_gradle_text(text: &str) -> ProjectInference {
    let base = extract_application_id(text).unwrap_or_default();
    let suffixes = extract_application_id_suffixes(text);
    let dimensions = extract_flavor_dimensions(text);

    // Map flavor_name → dimension
    let flavor_map = extract_flavors_with_dimensions(text, &dimensions);

    // All flavor names (flat, sorted)
    let mut flavor_names: Vec<String> = flavor_map.keys().cloned().collect();
    flavor_names.sort();
    flavor_names.dedup();

    // Per-dimension: pick first flavor (alphabetically)
    let mut selected_flavors: Vec<(String, String)> = Vec::new();
    if dimensions.is_empty() {
        // No explicit dimensions — just use all flavors unsorted
        if !flavor_names.is_empty() {
            selected_flavors.push(("".to_string(), flavor_names[0].clone()));
        }
    } else {
        for dim in &dimensions {
            let mut flavors_in_dim: Vec<&str> = flavor_map
                .iter()
                .filter(|(_, d)| d.as_str() == dim.as_str())
                .map(|(n, _)| n.as_str())
                .collect();
            flavors_in_dim.sort();
            if let Some(&first) = flavors_in_dim.first() {
                selected_flavors.push((dim.clone(), first.to_string()));
            }
        }
    }

    // Build variant name = camel-cased concatenation of selected flavors + "Debug"
    let variant_segment: String = selected_flavors
        .iter()
        .map(|(_, f)| capitalize(f))
        .collect();

    let (assemble_task, install_task, variant_summary) = if variant_segment.is_empty() {
        (
            Some("assembleDebug".to_string()),
            Some("installDebug".to_string()),
            "debug (no flavors)".to_string(),
        )
    } else {
        let a = format!("assemble{variant_segment}Debug");
        let i = format!("install{variant_segment}Debug");
        let summary = format!("{variant_segment}Debug");
        (Some(a), Some(i), summary)
    };

    // Build application_ids list
    let mut application_ids: Vec<String> = Vec::new();
    if !base.is_empty() {
        application_ids.push(base.clone());
        for suf in &suffixes {
            let id = join_suffix(&base, suf);
            if !application_ids.contains(&id) {
                application_ids.push(id);
            }
        }
    }
    // Also add suffix-less IDs derived from flavors that have applicationIdSuffix inside them
    // (already covered by `extract_application_id_suffixes` which scans the whole file)

    ProjectInference {
        application_ids,
        flavor_names,
        flavor_dimensions: dimensions,
        selected_flavors,
        assemble_task,
        install_task,
        variant_summary,
        gradle_file: None,
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn join_suffix(base: &str, suf: &str) -> String {
    if suf.is_empty() {
        base.to_string()
    } else if suf.starts_with('.') {
        format!("{base}{suf}")
    } else {
        format!("{base}.{suf}")
    }
}

// ─── Gradle DSL extraction ────────────────────────────────────────────────────

fn extract_application_id(text: &str) -> Option<String> {
    // Look inside defaultConfig block first
    let search = text
        .find("defaultConfig")
        .map(|i| &text[i..i + 8_000.min(text.len().saturating_sub(i))])
        .unwrap_or(text);

    for re_src in [
        r#"(?i)applicationId\s+["']([^"']+)["']"#,
        r#"(?i)applicationId\s*=\s*["']([^"']+)["']"#,
    ] {
        if let Ok(re) = Regex::new(re_src) {
            if let Some(c) = re.captures(search) {
                return c.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    None
}

fn extract_application_id_suffixes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for re_src in [
        r#"(?i)applicationIdSuffix\s*=\s*["']([^"']*)["']"#,
        r#"(?i)applicationIdSuffix\s+["']([^"']*)["']"#,
    ] {
        if let Ok(re) = Regex::new(re_src) {
            for c in re.captures_iter(text) {
                if let Some(m) = c.get(1) {
                    out.push(m.as_str().to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Parse `flavorDimensions "track", "environment"` or `flavorDimensions("track", "env")`.
fn extract_flavor_dimensions(text: &str) -> Vec<String> {
    // Groovy: flavorDimensions "a", "b"
    // Kotlin: flavorDimensions("a", "b") or flavorDimensions += listOf("a", "b")
    let re_src = r#"(?m)flavorDimensions\s*[\(+\s]*(?:listOf\s*\()?([\s\S]*?)[\)\n]"#;
    let Ok(re) = Regex::new(re_src) else {
        return Vec::new();
    };
    if let Some(c) = re.captures(text) {
        if let Some(m) = c.get(1) {
            let raw = m.as_str();
            let re_quoted = Regex::new(r#"["']([^"']+)["']"#).expect("regex");
            let mut dims: Vec<String> = re_quoted
                .captures_iter(raw)
                .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                .collect();
            dims.dedup();
            if !dims.is_empty() {
                return dims;
            }
        }
    }
    Vec::new()
}

/// Extract all named flavors and their dimension assignment.
/// Returns a map of flavor_name → dimension (empty string when unassigned).
fn extract_flavors_with_dimensions(text: &str, dimensions: &[String]) -> HashMap<String, String> {
    let Some(start) = text.find("productFlavors") else {
        return HashMap::new();
    };
    let slice = &text[start..start + 24_000.min(text.len().saturating_sub(start))];
    let mut map: HashMap<String, String> = HashMap::new();

    // Kotlin DSL: create("canary") { ... }
    let re_create = Regex::new(r#"create\s*\(\s*["']([^"']+)["']\s*\)"#).expect("regex");
    for c in re_create.captures_iter(slice) {
        if let Some(m) = c.get(1) {
            map.entry(m.as_str().to_string()).or_default();
        }
    }

    // Groovy DSL: canary { ... }  (a bare identifier followed by braces)
    const SKIP: &[&str] = &[
        "defaultConfig", "buildTypes", "create", "dimension", "productFlavors",
        "signingConfigs", "kotlinOptions", "packagingOptions", "packaging",
        "compileOptions", "buildFeatures", "android", "dependencies",
    ];
    let re_block = Regex::new(r"(?m)^\s{2,}([a-zA-Z_][a-zA-Z0-9_]*)\s*\{").expect("regex");
    for c in re_block.captures_iter(slice) {
        if let Some(m) = c.get(1) {
            let n = m.as_str();
            if !SKIP.contains(&n) {
                map.entry(n.to_string()).or_default();
            }
        }
    }

    if map.is_empty() || dimensions.is_empty() {
        return map;
    }

    // Now try to figure out which dimension each flavor belongs to by
    // finding `dimension "track"` inside each flavor's block.
    let re_dim_assign = Regex::new(r#"dimension\s*[=\s]*["']([^"']+)["']"#).expect("regex");

    // We'll do a naive linear scan: for each flavor name, find the text block
    // starting just after the flavor name and grab the first `dimension` assignment.
    for (flavor, assigned_dim) in map.iter_mut() {
        // Find the flavor in the slice
        if let Some(pos) = find_flavor_block_start(slice, flavor) {
            let block = &slice[pos..pos + 2_000.min(slice.len().saturating_sub(pos))];
            if let Some(c) = re_dim_assign.captures(block) {
                if let Some(d) = c.get(1) {
                    *assigned_dim = d.as_str().to_string();
                }
            }
        }
    }
    map
}

/// Find the start of the block body for a given flavor name (past the `{`).
fn find_flavor_block_start(text: &str, name: &str) -> Option<usize> {
    // Pattern: `<name> {` or `create("name") {`
    let pattern1 = format!("{name} {{");
    let pattern2 = format!("{name}{{");
    let pattern3 = format!(r#"("{name}")"#);
    for pattern in [&pattern1, &pattern2, &pattern3] {
        if let Some(pos) = text.find(pattern.as_str()) {
            // Advance past the opening brace
            if let Some(brace) = text[pos..].find('{') {
                return Some(pos + brace + 1);
            }
        }
    }
    None
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_dimension_suffix() {
        let text = r#"
        android {
            defaultConfig { applicationId "ai.wayve.app" }
            productFlavors {
                dev {
                    applicationIdSuffix ".dev"
                    dimension "env"
                }
            }
        }
        "#;
        let inf = infer_from_gradle_text(text);
        assert!(inf.application_ids.contains(&"ai.wayve.app".to_string()));
        assert!(inf.application_ids.contains(&"ai.wayve.app.dev".to_string()));
        assert_eq!(inf.assemble_task.as_deref(), Some("assembleDevDebug"));
    }

    #[test]
    fn no_flavors() {
        let text = r#"android { defaultConfig { applicationId "com.example.app" } }"#;
        let inf = infer_from_gradle_text(text);
        assert_eq!(inf.application_ids, vec!["com.example.app"]);
        assert_eq!(inf.assemble_task.as_deref(), Some("assembleDebug"));
    }

    #[test]
    fn two_dimensions_canary_dev() {
        let text = r#"
        android {
            defaultConfig { applicationId "ai.wayve.app" }
            flavorDimensions "track", "environment"
            productFlavors {
                canary {
                    dimension "track"
                    applicationIdSuffix ".canary"
                }
                stable {
                    dimension "track"
                }
                dev {
                    dimension "environment"
                    applicationIdSuffix ".dev"
                }
                prod {
                    dimension "environment"
                }
            }
        }
        "#;
        let inf = infer_from_gradle_text(text);
        // Dimensions order: ["track", "environment"]
        // First of track (sorted): canary, stable → canary
        // First of environment (sorted): dev, prod → dev
        // Variant: canaryDevDebug
        assert_eq!(inf.assemble_task.as_deref(), Some("assembleCanaryDevDebug"));
        assert_eq!(inf.install_task.as_deref(), Some("installCanaryDevDebug"));
        assert!(inf.application_ids.contains(&"ai.wayve.app".to_string()));
    }
}
