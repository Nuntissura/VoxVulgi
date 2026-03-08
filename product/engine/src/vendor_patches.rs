use crate::persistence::atomic_write_text;
use crate::{EngineError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchOutcome {
    AlreadyPatched,
    Noop,
    Patched,
}

pub fn patch_webrtcvad_pkg_resources_import(python: &Path) -> Result<bool> {
    let file_path = site_package_file_from_venv_python(python, &["webrtcvad.py"])?;
    let original = std::fs::read_to_string(&file_path).map_err(|e| {
        EngineError::InstallFailed(format!("failed to read {}: {e}", file_path.display()))
    })?;
    let (patched, outcome) = transform_webrtcvad_text(&original);
    if matches!(outcome, PatchOutcome::Patched) {
        atomic_write_text(&file_path, &patched).map_err(|e| {
            EngineError::InstallFailed(format!("failed to patch {}: {e}", file_path.display()))
        })?;
    }
    Ok(matches!(
        outcome,
        PatchOutcome::Patched | PatchOutcome::AlreadyPatched
    ))
}

pub fn webrtcvad_patch_applied(python: &Path) -> Option<bool> {
    let file_path = site_package_file_from_venv_python(python, &["webrtcvad.py"]).ok()?;
    let text = std::fs::read_to_string(file_path).ok()?;
    let (_, outcome) = transform_webrtcvad_text(&text);
    Some(matches!(
        outcome,
        PatchOutcome::Patched | PatchOutcome::AlreadyPatched
    ))
}

pub fn patch_openvoice_api_enable_watermark(python: &Path) -> Result<bool> {
    let file_path = site_package_file_from_venv_python(python, &["openvoice", "api.py"])?;
    let original = std::fs::read_to_string(&file_path).map_err(|e| {
        EngineError::InstallFailed(format!("failed to read {}: {e}", file_path.display()))
    })?;
    let (patched, outcome) = transform_openvoice_api_text(&original)?;
    if matches!(outcome, PatchOutcome::Patched) {
        atomic_write_text(&file_path, &patched).map_err(|e| {
            EngineError::InstallFailed(format!("failed to patch {}: {e}", file_path.display()))
        })?;
    }
    Ok(matches!(
        outcome,
        PatchOutcome::Patched | PatchOutcome::AlreadyPatched
    ))
}

pub fn openvoice_api_patch_applied(python: &Path) -> Option<bool> {
    let file_path = site_package_file_from_venv_python(python, &["openvoice", "api.py"]).ok()?;
    let text = std::fs::read_to_string(file_path).ok()?;
    Some(openvoice_api_patch_applied_text(&text))
}

fn transform_webrtcvad_text(text: &str) -> (String, PatchOutcome) {
    if text.contains("try:")
        && text.contains("import pkg_resources")
        && text.contains("if pkg_resources else 'installed'")
    {
        return (text.to_string(), PatchOutcome::AlreadyPatched);
    }
    if !text.contains("import pkg_resources") {
        return (text.to_string(), PatchOutcome::Noop);
    }

    let replaced_import = text.replace(
        "import pkg_resources\n\nimport _webrtcvad\n",
        "try:\n    import pkg_resources\nexcept Exception:  # pragma: no cover\n    pkg_resources = None\n\nimport _webrtcvad\n",
    );
    let replaced_version = replaced_import.replace(
        "__version__ = pkg_resources.get_distribution('webrtcvad').version",
        "__version__ = (pkg_resources.get_distribution('webrtcvad').version if pkg_resources else 'installed')",
    );

    if replaced_version == text {
        return (text.to_string(), PatchOutcome::Noop);
    }

    (replaced_version, PatchOutcome::Patched)
}

fn transform_openvoice_api_text(text: &str) -> Result<(String, PatchOutcome)> {
    if openvoice_api_patch_applied_text(text) {
        return Ok((text.to_string(), PatchOutcome::AlreadyPatched));
    }

    let broken_newline = "enable_watermark = kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)";
    if text.contains(broken_newline) {
        let mut patched = text.replacen(
            broken_newline,
            "enable_watermark = kwargs.pop('enable_watermark', True)\n        super().__init__(*args, **kwargs)",
            1,
        );
        if patched.contains("if kwargs.get('enable_watermark', True):") {
            patched = patched.replacen(
                "if kwargs.get('enable_watermark', True):",
                "if enable_watermark:",
                1,
            );
        }
        return Ok((patched, PatchOutcome::Patched));
    }

    if !text.contains("super().__init__(*args, **kwargs)") {
        return Err(EngineError::InstallFailed(
            "unexpected openvoice/api.py: missing super().__init__ call".to_string(),
        ));
    }
    if !text.contains("if kwargs.get('enable_watermark', True):") {
        return Err(EngineError::InstallFailed(
            "unexpected openvoice/api.py: missing enable_watermark condition".to_string(),
        ));
    }

    let mut patched = text.replacen(
        "super().__init__(*args, **kwargs)",
        "enable_watermark = kwargs.pop('enable_watermark', True)\n        super().__init__(*args, **kwargs)",
        1,
    );
    patched = patched.replacen(
        "if kwargs.get('enable_watermark', True):",
        "if enable_watermark:",
        1,
    );
    Ok((patched, PatchOutcome::Patched))
}

fn openvoice_api_patch_applied_text(text: &str) -> bool {
    let has_pop = text.contains("kwargs.pop('enable_watermark'")
        || text.contains("kwargs.pop(\"enable_watermark\"");
    let has_if_enable = text.contains("if enable_watermark:");
    let broken = text.contains(
        "kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)",
    ) || text.contains(
        "kwargs.pop(\"enable_watermark\", True)\\\\n        super().__init__(*args, **kwargs)",
    ) || text.contains(
        "enable_watermark = kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)",
    ) || text.contains(
        "enable_watermark = kwargs.pop(\"enable_watermark\", True)\\\\n        super().__init__(*args, **kwargs)",
    );

    has_pop && has_if_enable && !broken
}

fn site_package_file_from_venv_python(python: &Path, relative: &[&str]) -> Result<PathBuf> {
    let venv_dir = python
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| EngineError::InstallFailed("invalid venv python path".to_string()))?;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let mut path = venv_dir.join("Lib").join("site-packages");
        for segment in relative {
            path = path.join(segment);
        }
        candidates.push(path);
    } else {
        let lib_dir = venv_dir.join("lib");
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with("python") {
                    continue;
                }
                let mut candidate = path.join("site-packages");
                for segment in relative {
                    candidate = candidate.join(segment);
                }
                candidates.push(candidate);
            }
        }
    }

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "site-package file not found: {}",
                relative.join("/")
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webrtcvad_patch_is_idempotent() {
        let original = "import pkg_resources\n\nimport _webrtcvad\n\n__version__ = pkg_resources.get_distribution('webrtcvad').version\n";
        let (patched, first) = transform_webrtcvad_text(original);
        assert_eq!(first, PatchOutcome::Patched);
        assert!(patched.contains("try:\n    import pkg_resources"));
        assert!(patched.contains("if pkg_resources else 'installed'"));

        let (again, second) = transform_webrtcvad_text(&patched);
        assert_eq!(second, PatchOutcome::AlreadyPatched);
        assert_eq!(patched, again);
    }

    #[test]
    fn openvoice_patch_is_idempotent() {
        let original = "class ToneColorConverter:\n    def __init__(self, *args, **kwargs):\n        super().__init__(*args, **kwargs)\n        if kwargs.get('enable_watermark', True):\n            pass\n";
        let (patched, first) = transform_openvoice_api_text(original).expect("patch");
        assert_eq!(first, PatchOutcome::Patched);
        assert!(openvoice_api_patch_applied_text(&patched));

        let (again, second) = transform_openvoice_api_text(&patched).expect("already patched");
        assert_eq!(second, PatchOutcome::AlreadyPatched);
        assert_eq!(patched, again);
    }

    #[test]
    fn openvoice_patch_repairs_broken_newline_variant() {
        let broken = "class ToneColorConverter:\n    def __init__(self, *args, **kwargs):\n        enable_watermark = kwargs.pop('enable_watermark', True)\\\\n        super().__init__(*args, **kwargs)\n        if kwargs.get('enable_watermark', True):\n            pass\n";
        let (patched, outcome) = transform_openvoice_api_text(broken).expect("patch");
        assert_eq!(outcome, PatchOutcome::Patched);
        assert!(patched.contains("enable_watermark = kwargs.pop('enable_watermark', True)\n        super().__init__(*args, **kwargs)"));
        assert!(patched.contains("if enable_watermark:"));
    }
}
