use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterTemplate {
    pub backend_id: String,
    pub display_name: String,
    pub expected_markers: Vec<String>,
    pub default_entry_command: Vec<String>,
    pub probe_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterConfig {
    pub backend_id: String,
    pub enabled: bool,
    pub root_dir: Option<String>,
    pub python_exe: Option<String>,
    pub model_dir: Option<String>,
    pub entry_command: Vec<String>,
    pub probe_command: Vec<String>,
    pub notes: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterProbe {
    pub backend_id: String,
    pub ready: bool,
    pub status: String,
    pub summary: String,
    pub checked_at_ms: i64,
    pub root_exists: bool,
    pub python_exists: bool,
    pub model_dir_exists: bool,
    pub entry_exists: bool,
    pub markers_found: Vec<String>,
    pub missing_markers: Vec<String>,
    pub command_exit_code: Option<i32>,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterDetail {
    pub template: VoiceBackendAdapterTemplate,
    pub config: Option<VoiceBackendAdapterConfig>,
    pub last_probe: Option<VoiceBackendAdapterProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceBackendAdapterStore {
    adapters: Vec<VoiceBackendAdapterConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceBackendAdapterProbeStore {
    probes: Vec<VoiceBackendAdapterProbe>,
}

pub fn adapter_templates() -> Vec<VoiceBackendAdapterTemplate> {
    vec![
        VoiceBackendAdapterTemplate {
            backend_id: "cosyvoice".to_string(),
            display_name: "CosyVoice".to_string(),
            expected_markers: vec!["webui.py".to_string(), "requirements.txt".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "webui.py".to_string()],
            probe_hint: "Point root_dir at a local CosyVoice checkout or packaged environment."
                .to_string(),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "seed_vc".to_string(),
            display_name: "Seed-VC".to_string(),
            expected_markers: vec!["app.py".to_string(), "requirements.txt".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "app.py".to_string()],
            probe_hint: "Point root_dir at a local Seed-VC checkout with its runtime already prepared."
                .to_string(),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "indextts2".to_string(),
            display_name: "IndexTTS2".to_string(),
            expected_markers: vec!["README.md".to_string(), "requirements.txt".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "infer.py".to_string()],
            probe_hint: "Point root_dir at an IndexTTS/IndexTTS2 checkout and override entry_command if needed."
                .to_string(),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "fish_speech".to_string(),
            display_name: "Fish-Speech".to_string(),
            expected_markers: vec!["pyproject.toml".to_string(), "fish_speech".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "-m".to_string(), "fish_speech".to_string()],
            probe_hint: "Point root_dir at a local Fish-Speech checkout or env and set a lightweight probe command."
                .to_string(),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "xtts_v2".to_string(),
            display_name: "XTTS v2".to_string(),
            expected_markers: vec!["TTS".to_string(), "README.md".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "-m".to_string(), "TTS.server.server".to_string()],
            probe_hint: "Point root_dir at a Coqui-TTS environment if you want XTTS v2 available for experiments."
                .to_string(),
        },
    ]
}

pub fn list_voice_backend_adapters(paths: &AppPaths) -> Result<Vec<VoiceBackendAdapterDetail>> {
    let configs = load_configs(paths)?;
    let probes = load_probes(paths)?;
    Ok(merge_templates(configs, probes))
}

pub fn upsert_voice_backend_adapter(
    paths: &AppPaths,
    mut config: VoiceBackendAdapterConfig,
) -> Result<VoiceBackendAdapterDetail> {
    let template = template_by_backend_id(&config.backend_id)?;
    config.backend_id = template.backend_id.clone();
    config.updated_at_ms = now_ms();
    let mut configs = load_configs(paths)?;
    if let Some(existing) = configs.iter_mut().find(|value| value.backend_id == config.backend_id) {
        *existing = config.clone();
    } else {
        configs.push(config.clone());
    }
    save_configs(paths, &configs)?;
    Ok(merge_templates(configs, load_probes(paths)?)
        .into_iter()
        .find(|detail| detail.template.backend_id == config.backend_id)
        .expect("adapter detail after upsert"))
}

pub fn delete_voice_backend_adapter(paths: &AppPaths, backend_id: &str) -> Result<()> {
    let backend_id = normalize_backend_id(backend_id);
    let mut configs = load_configs(paths)?;
    configs.retain(|value| value.backend_id != backend_id);
    save_configs(paths, &configs)?;
    let mut probes = load_probes(paths)?;
    probes.retain(|value| value.backend_id != backend_id);
    save_probes(paths, &probes)?;
    Ok(())
}

pub fn probe_voice_backend_adapter(
    paths: &AppPaths,
    backend_id: &str,
) -> Result<VoiceBackendAdapterDetail> {
    let backend_id = normalize_backend_id(backend_id);
    let config = load_configs(paths)?
        .into_iter()
        .find(|value| value.backend_id == backend_id)
        .ok_or_else(|| EngineError::InstallFailed(format!("adapter config not found: {backend_id}")))?;
    let template = template_by_backend_id(&backend_id)?;
    let probe = run_probe(&template, &config)?;

    let mut probes = load_probes(paths)?;
    if let Some(existing) = probes.iter_mut().find(|value| value.backend_id == backend_id) {
        *existing = probe.clone();
    } else {
        probes.push(probe);
    }
    save_probes(paths, &probes)?;
    Ok(VoiceBackendAdapterDetail {
        template,
        config: Some(config),
        last_probe: probes
            .into_iter()
            .find(|value| value.backend_id == backend_id),
    })
}

pub fn catalog_status_overrides(paths: &AppPaths) -> Result<HashMap<String, (String, String)>> {
    let details = list_voice_backend_adapters(paths)?;
    let mut out = HashMap::new();
    for detail in details {
        let backend_id = detail.template.backend_id.clone();
        let value = match (&detail.config, &detail.last_probe) {
            (Some(config), Some(probe)) if config.enabled && probe.ready => (
                "byo_ready".to_string(),
                format!(
                    "BYO adapter is configured and last probe passed at {}.",
                    probe.checked_at_ms
                ),
            ),
            (Some(config), Some(probe)) if config.enabled => (
                "byo_probe_failed".to_string(),
                format!("BYO adapter is configured but last probe failed: {}", probe.summary),
            ),
            (Some(config), None) if config.enabled => (
                "byo_configured_unprobed".to_string(),
                "BYO adapter is configured but has not been probed yet.".to_string(),
            ),
            (Some(_), _) => (
                "byo_configured_disabled".to_string(),
                "BYO adapter is configured but disabled.".to_string(),
            ),
            (None, _) => continue,
        };
        out.insert(backend_id, value);
    }
    Ok(out)
}

fn merge_templates(
    configs: Vec<VoiceBackendAdapterConfig>,
    probes: Vec<VoiceBackendAdapterProbe>,
) -> Vec<VoiceBackendAdapterDetail> {
    let config_map = configs
        .into_iter()
        .map(|value| (value.backend_id.clone(), value))
        .collect::<HashMap<_, _>>();
    let probe_map = probes
        .into_iter()
        .map(|value| (value.backend_id.clone(), value))
        .collect::<HashMap<_, _>>();
    adapter_templates()
        .into_iter()
        .map(|template| VoiceBackendAdapterDetail {
            config: config_map.get(&template.backend_id).cloned(),
            last_probe: probe_map.get(&template.backend_id).cloned(),
            template,
        })
        .collect()
}

fn run_probe(
    template: &VoiceBackendAdapterTemplate,
    config: &VoiceBackendAdapterConfig,
) -> Result<VoiceBackendAdapterProbe> {
    let root_dir = config.root_dir.as_deref().map(PathBuf::from);
    let root_exists = root_dir.as_ref().map(|value| value.is_dir()).unwrap_or(false);
    let python_exists = config
        .python_exe
        .as_deref()
        .map(PathBuf::from)
        .map(|value| value.exists())
        .unwrap_or(false);
    let model_dir_exists = config
        .model_dir
        .as_deref()
        .map(PathBuf::from)
        .map(|value| value.exists())
        .unwrap_or(false);

    let entry_tokens = if config.entry_command.is_empty() {
        template.default_entry_command.clone()
    } else {
        config.entry_command.clone()
    };
    let entry_exists = command_target_exists(root_dir.as_deref(), &entry_tokens, config.python_exe.as_deref());

    let mut markers_found = Vec::new();
    let mut missing_markers = Vec::new();
    if let Some(root) = root_dir.as_ref() {
        for marker in &template.expected_markers {
            let marker_path = root.join(marker);
            if marker_path.exists() {
                markers_found.push(marker.clone());
            } else {
                missing_markers.push(marker.clone());
            }
        }
    } else {
        missing_markers = template.expected_markers.clone();
    }

    let mut messages = Vec::new();
    if !root_exists {
        messages.push("Configured root_dir is missing.".to_string());
    }
    if config.python_exe.is_some() && !python_exists {
        messages.push("Configured python_exe is missing.".to_string());
    }
    if config.model_dir.is_some() && !model_dir_exists {
        messages.push("Configured model_dir is missing.".to_string());
    }
    if !entry_exists {
        messages.push("Entry command target could not be resolved from the current config.".to_string());
    }

    let mut command_exit_code = None;
    let mut stdout_preview = None;
    let mut stderr_preview = None;
    if !config.probe_command.is_empty() {
        match run_probe_command(root_dir.as_deref(), &config.probe_command, config.python_exe.as_deref()) {
            Ok((code, stdout, stderr)) => {
                command_exit_code = code;
                stdout_preview = stdout;
                stderr_preview = stderr;
                if code != Some(0) {
                    messages.push("Probe command returned a non-zero exit code.".to_string());
                }
            }
            Err(error) => messages.push(error),
        }
    }

    let ready = config.enabled
        && root_exists
        && missing_markers.is_empty()
        && entry_exists
        && messages.is_empty();
    let status = if ready {
        "ready"
    } else if !config.enabled {
        "disabled"
    } else {
        "error"
    };
    let summary = if ready {
        format!("{} probe passed.", template.display_name)
    } else if !config.enabled {
        format!("{} adapter is disabled.", template.display_name)
    } else {
        messages.join(" ")
    };

    Ok(VoiceBackendAdapterProbe {
        backend_id: config.backend_id.clone(),
        ready,
        status: status.to_string(),
        summary,
        checked_at_ms: now_ms(),
        root_exists,
        python_exists,
        model_dir_exists,
        entry_exists,
        markers_found,
        missing_markers,
        command_exit_code,
        stdout_preview,
        stderr_preview,
        messages,
    })
}

fn command_target_exists(root_dir: Option<&Path>, tokens: &[String], python_exe: Option<&str>) -> bool {
    let Some(first) = tokens.first() else {
        return false;
    };
    let first = expand_token(first, root_dir, python_exe, None);
    if first.is_empty() {
        return false;
    }
    if first == "python" || first == "python3" || first == "py" {
        return true;
    }
    let path = PathBuf::from(&first);
    path.exists() || root_dir.map(|root| root.join(&first).exists()).unwrap_or(false)
}

fn run_probe_command(
    root_dir: Option<&Path>,
    tokens: &[String],
    python_exe: Option<&str>,
) -> std::result::Result<(Option<i32>, Option<String>, Option<String>), String> {
    let Some(first) = tokens.first() else {
        return Err("Probe command is empty.".to_string());
    };
    let model_dir = None;
    let program = resolve_program_token(&expand_token(first, root_dir, python_exe, model_dir), root_dir);
    let mut command = Command::new(&program);
    if let Some(root) = root_dir {
        command.current_dir(root);
    }
    for token in tokens.iter().skip(1) {
        command.arg(expand_token(token, root_dir, python_exe, model_dir));
    }
    let output = command.output().map_err(|e| format!("Probe command failed to start: {e}"))?;
    Ok((
        output.status.code(),
        Some(truncate_output(String::from_utf8_lossy(&output.stdout).trim())),
        Some(truncate_output(String::from_utf8_lossy(&output.stderr).trim())),
    ))
}

fn expand_token(
    token: &str,
    root_dir: Option<&Path>,
    python_exe: Option<&str>,
    model_dir: Option<&str>,
) -> String {
    match token {
        "{python_exe}" => python_exe.unwrap_or("python").to_string(),
        "{root_dir}" => root_dir
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default(),
        "{model_dir}" => model_dir.unwrap_or_default().to_string(),
        other => other.to_string(),
    }
}

fn resolve_program_token(token: &str, root_dir: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(token);
    if path.is_absolute() || path.exists() {
        return path;
    }
    if let Some(root) = root_dir {
        let joined = root.join(token);
        if joined.exists() {
            return joined;
        }
    }
    PathBuf::from(token)
}

fn truncate_output(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 240 {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..237])
    }
}

fn template_by_backend_id(backend_id: &str) -> Result<VoiceBackendAdapterTemplate> {
    let backend_id = normalize_backend_id(backend_id);
    adapter_templates()
        .into_iter()
        .find(|template| template.backend_id == backend_id)
        .ok_or_else(|| EngineError::InstallFailed(format!("unsupported BYO backend id: {backend_id}")))
}

fn normalize_backend_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn load_configs(paths: &AppPaths) -> Result<Vec<VoiceBackendAdapterConfig>> {
    let path = paths.voice_backend_adapters_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let parsed = serde_json::from_slice::<VoiceBackendAdapterStore>(&bytes)?;
    Ok(parsed.adapters)
}

fn save_configs(paths: &AppPaths, configs: &[VoiceBackendAdapterConfig]) -> Result<()> {
    std::fs::create_dir_all(paths.config_dir())?;
    let store = VoiceBackendAdapterStore {
        adapters: configs.to_vec(),
    };
    let json = serde_json::to_string_pretty(&store)?;
    std::fs::write(paths.voice_backend_adapters_path(), format!("{json}\n"))?;
    Ok(())
}

fn load_probes(paths: &AppPaths) -> Result<Vec<VoiceBackendAdapterProbe>> {
    let path = paths.voice_backend_adapter_probes_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let parsed = serde_json::from_slice::<VoiceBackendAdapterProbeStore>(&bytes)?;
    Ok(parsed.probes)
}

fn save_probes(paths: &AppPaths, probes: &[VoiceBackendAdapterProbe]) -> Result<()> {
    std::fs::create_dir_all(paths.config_dir())?;
    let store = VoiceBackendAdapterProbeStore {
        probes: probes.to_vec(),
    };
    let json = serde_json::to_string_pretty(&store)?;
    std::fs::write(paths.voice_backend_adapter_probes_path(), format!("{json}\n"))?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_list_adapter_round_trips_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let detail = upsert_voice_backend_adapter(
            &paths,
            VoiceBackendAdapterConfig {
                backend_id: "cosyvoice".to_string(),
                enabled: true,
                root_dir: Some(dir.path().to_string_lossy().to_string()),
                python_exe: None,
                model_dir: None,
                entry_command: vec![],
                probe_command: vec![],
                notes: Some("test".to_string()),
                updated_at_ms: 0,
            },
        )
        .expect("upsert");
        assert_eq!(detail.template.backend_id, "cosyvoice");
        let listed = list_voice_backend_adapters(&paths).expect("list");
        assert!(listed.iter().any(|value| value.config.is_some()));
    }

    #[test]
    fn probe_adapter_with_local_mock_command_reports_ready() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        std::fs::write(dir.path().join("webui.py"), "print('ok')\n").expect("marker");
        std::fs::write(dir.path().join("requirements.txt"), "ok\n").expect("marker2");
        let probe_command = if cfg!(windows) {
            vec!["cmd".to_string(), "/C".to_string(), "echo ok".to_string()]
        } else {
            vec!["/bin/sh".to_string(), "-c".to_string(), "echo ok".to_string()]
        };
        upsert_voice_backend_adapter(
            &paths,
            VoiceBackendAdapterConfig {
                backend_id: "cosyvoice".to_string(),
                enabled: true,
                root_dir: Some(dir.path().to_string_lossy().to_string()),
                python_exe: None,
                model_dir: None,
                entry_command: vec!["{python_exe}".to_string(), "webui.py".to_string()],
                probe_command,
                notes: None,
                updated_at_ms: 0,
            },
        )
        .expect("upsert");
        let detail = probe_voice_backend_adapter(&paths, "cosyvoice").expect("probe");
        assert_eq!(detail.last_probe.as_ref().map(|value| value.ready), Some(true));
    }
}
