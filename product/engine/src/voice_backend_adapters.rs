use crate::paths::AppPaths;
use crate::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendStarterRecipe {
    pub recipe_id: String,
    pub display_name: String,
    pub description: String,
    pub suggested_model_dir: Option<String>,
    pub default_entry_command: Vec<String>,
    pub default_probe_command: Vec<String>,
    pub default_render_command: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterTemplate {
    pub backend_id: String,
    pub display_name: String,
    pub expected_markers: Vec<String>,
    pub default_entry_command: Vec<String>,
    pub probe_hint: String,
    pub starter_recipes: Vec<VoiceBackendStarterRecipe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterConfig {
    pub backend_id: String,
    pub enabled: bool,
    pub root_dir: Option<String>,
    pub python_exe: Option<String>,
    pub model_dir: Option<String>,
    #[serde(default)]
    pub entry_command: Vec<String>,
    #[serde(default)]
    pub probe_command: Vec<String>,
    #[serde(default)]
    pub render_command: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendAdapterResolvedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub current_dir: Option<String>,
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
            starter_recipes: starter_recipes_for_backend("cosyvoice"),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "seed_vc".to_string(),
            display_name: "Seed-VC".to_string(),
            expected_markers: vec!["app.py".to_string(), "requirements.txt".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "app.py".to_string()],
            probe_hint: "Point root_dir at a local Seed-VC checkout with its runtime already prepared."
                .to_string(),
            starter_recipes: starter_recipes_for_backend("seed_vc"),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "indextts2".to_string(),
            display_name: "IndexTTS2".to_string(),
            expected_markers: vec!["README.md".to_string(), "requirements.txt".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "infer.py".to_string()],
            probe_hint: "Point root_dir at an IndexTTS/IndexTTS2 checkout and override entry_command if needed."
                .to_string(),
            starter_recipes: starter_recipes_for_backend("indextts2"),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "fish_speech".to_string(),
            display_name: "Fish-Speech".to_string(),
            expected_markers: vec!["pyproject.toml".to_string(), "fish_speech".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "-m".to_string(), "fish_speech".to_string()],
            probe_hint: "Point root_dir at a local Fish-Speech checkout or env and set a lightweight probe command."
                .to_string(),
            starter_recipes: starter_recipes_for_backend("fish_speech"),
        },
        VoiceBackendAdapterTemplate {
            backend_id: "xtts_v2".to_string(),
            display_name: "XTTS v2".to_string(),
            expected_markers: vec!["TTS".to_string(), "README.md".to_string()],
            default_entry_command: vec!["{python_exe}".to_string(), "-m".to_string(), "TTS.server.server".to_string()],
            probe_hint: "Point root_dir at a Coqui-TTS environment if you want XTTS v2 available for experiments."
                .to_string(),
            starter_recipes: starter_recipes_for_backend("xtts_v2"),
        },
    ]
}

fn starter_recipes_for_backend(backend_id: &str) -> Vec<VoiceBackendStarterRecipe> {
    match normalize_backend_id(backend_id).as_str() {
        "cosyvoice" => vec![
            starter_recipe(
                "cosyvoice_python_wrapper",
                "Repo-local Python wrapper",
                "Best default when you keep a lightweight VoxVulgi render wrapper inside the CosyVoice checkout.",
                Some("pretrained_models"),
                vec!["{python_exe}", "webui.py"],
                vec!["{python_exe}", "--version"],
                vec![
                    "{python_exe}",
                    "voxvulgi_cosyvoice_render.py",
                    "--request",
                    "{request_json}",
                    "--manifest",
                    "{manifest_json}",
                    "--report",
                    "{report_json}",
                    "--output-dir",
                    "{output_dir}",
                    "--backend",
                    "{backend_id}",
                    "--track",
                    "{track_id}",
                    "--model-dir",
                    "{model_dir}",
                ],
                vec![
                    "Point root_dir at the CosyVoice checkout or packaged environment.".to_string(),
                    "Keep the wrapper in that checkout so local imports and weights resolve naturally.".to_string(),
                    "The wrapper must read request.json and write both manifest.json and report.json.".to_string(),
                ],
            ),
        ],
        "seed_vc" => vec![
            starter_recipe(
                "seed_vc_python_wrapper",
                "Seed-VC wrapper pipeline",
                "Good baseline when Seed-VC is used as a conversion stage driven by a small local wrapper script.",
                Some("checkpoints"),
                vec!["{python_exe}", "app.py"],
                vec!["{python_exe}", "--version"],
                vec![
                    "{python_exe}",
                    "voxvulgi_seed_vc_render.py",
                    "--request",
                    "{request_json}",
                    "--manifest",
                    "{manifest_json}",
                    "--report",
                    "{report_json}",
                    "--output-dir",
                    "{output_dir}",
                    "--backend",
                    "{backend_id}",
                    "--track",
                    "{track_id}",
                    "--model-dir",
                    "{model_dir}",
                ],
                vec![
                    "Use this when the wrapper handles base TTS plus Seed-VC conversion in one local script.".to_string(),
                    "Keep model_dir pointed at the Seed-VC checkpoints directory if your wrapper expects it.".to_string(),
                    "This is usually strongest for identity transfer, so test it in the benchmark lab against expressive TTS candidates.".to_string(),
                ],
            ),
        ],
        "indextts2" => vec![
            starter_recipe(
                "indextts2_infer_wrapper",
                "IndexTTS2 inference wrapper",
                "Targets dubbing-style timing control with a repo-local inference wrapper.",
                Some("checkpoints"),
                vec!["{python_exe}", "infer.py"],
                vec!["{python_exe}", "--version"],
                vec![
                    "{python_exe}",
                    "voxvulgi_indextts2_render.py",
                    "--request",
                    "{request_json}",
                    "--manifest",
                    "{manifest_json}",
                    "--report",
                    "{report_json}",
                    "--output-dir",
                    "{output_dir}",
                    "--backend",
                    "{backend_id}",
                    "--track",
                    "{track_id}",
                    "--model-dir",
                    "{model_dir}",
                ],
                vec![
                    "Use this when IndexTTS2 is set up as a local inference checkout rather than a web service.".to_string(),
                    "The wrapper should keep timing-fit information from VoxVulgi's request segments when selecting durations.".to_string(),
                ],
            ),
        ],
        "fish_speech" => vec![
            starter_recipe(
                "fish_speech_module_wrapper",
                "Fish-Speech module wrapper",
                "Bootstraps a Fish-Speech checkout or virtual environment with a thin render wrapper.",
                Some("checkpoints"),
                vec!["{python_exe}", "-m", "fish_speech"],
                vec!["{python_exe}", "--version"],
                vec![
                    "{python_exe}",
                    "voxvulgi_fish_speech_render.py",
                    "--request",
                    "{request_json}",
                    "--manifest",
                    "{manifest_json}",
                    "--report",
                    "{report_json}",
                    "--output-dir",
                    "{output_dir}",
                    "--backend",
                    "{backend_id}",
                    "--track",
                    "{track_id}",
                    "--model-dir",
                    "{model_dir}",
                ],
                vec![
                    "Prefer this when you want stronger long-form expressiveness experiments.".to_string(),
                    "The wrapper should keep speaker-to-reference mapping explicit because Fish-Speech setups vary a lot by checkout.".to_string(),
                ],
            ),
        ],
        "xtts_v2" => vec![
            starter_recipe(
                "xtts_v2_python_wrapper",
                "XTTS v2 wrapper",
                "Practical starter for Coqui/XTTS-style local environments with a repo-local render wrapper.",
                Some("tts_models"),
                vec!["{python_exe}", "-m", "TTS.server.server"],
                vec!["{python_exe}", "--version"],
                vec![
                    "{python_exe}",
                    "voxvulgi_xtts_render.py",
                    "--request",
                    "{request_json}",
                    "--manifest",
                    "{manifest_json}",
                    "--report",
                    "{report_json}",
                    "--output-dir",
                    "{output_dir}",
                    "--backend",
                    "{backend_id}",
                    "--track",
                    "{track_id}",
                    "--model-dir",
                    "{model_dir}",
                ],
                vec![
                    "Use XTTS v2 as a practical comparison baseline when newer research backends are unstable.".to_string(),
                    "Keep model_dir pointed at the XTTS model root or leave it empty if your wrapper resolves models elsewhere.".to_string(),
                ],
            ),
        ],
        _ => Vec::new(),
    }
}

fn starter_recipe(
    recipe_id: &str,
    display_name: &str,
    description: &str,
    suggested_model_dir: Option<&str>,
    default_entry_command: Vec<&str>,
    default_probe_command: Vec<&str>,
    default_render_command: Vec<&str>,
    notes: Vec<String>,
) -> VoiceBackendStarterRecipe {
    VoiceBackendStarterRecipe {
        recipe_id: recipe_id.to_string(),
        display_name: display_name.to_string(),
        description: description.to_string(),
        suggested_model_dir: suggested_model_dir.map(|value| value.to_string()),
        default_entry_command: default_entry_command
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        default_probe_command: default_probe_command
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        default_render_command: default_render_command
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        notes,
    }
}

pub fn list_voice_backend_adapters(paths: &AppPaths) -> Result<Vec<VoiceBackendAdapterDetail>> {
    let configs = load_configs(paths)?;
    let probes = load_probes(paths)?;
    Ok(merge_templates(configs, probes))
}

pub fn get_voice_backend_adapter_detail(
    paths: &AppPaths,
    backend_id: &str,
) -> Result<VoiceBackendAdapterDetail> {
    let backend_id = normalize_backend_id(backend_id);
    list_voice_backend_adapters(paths)?
        .into_iter()
        .find(|detail| detail.template.backend_id == backend_id)
        .ok_or_else(|| {
            EngineError::InstallFailed(format!("unsupported BYO backend id: {backend_id}"))
        })
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

pub fn apply_voice_backend_starter_recipe(
    mut config: VoiceBackendAdapterConfig,
    recipe_id: &str,
) -> Result<VoiceBackendAdapterConfig> {
    let template = template_by_backend_id(&config.backend_id)?;
    let recipe = template
        .starter_recipes
        .iter()
        .find(|value| value.recipe_id == recipe_id.trim())
        .cloned()
        .ok_or_else(|| {
            EngineError::InstallFailed(format!(
                "starter recipe not found for {}: {}",
                template.backend_id, recipe_id
            ))
        })?;
    config.backend_id = template.backend_id;
    config.enabled = true;
    config.entry_command = recipe.default_entry_command.clone();
    config.probe_command = recipe.default_probe_command.clone();
    config.render_command = recipe.default_render_command.clone();
    if config.model_dir.as_deref().map(str::trim).unwrap_or("").is_empty() {
        config.model_dir = recipe.suggested_model_dir.clone();
    }
    if config.notes.as_deref().map(str::trim).unwrap_or("").is_empty() {
        config.notes = Some(format!(
            "Starter recipe: {}. {}",
            recipe.display_name, recipe.description
        ));
    }
    config.updated_at_ms = now_ms();
    Ok(config)
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

pub fn resolve_voice_backend_adapter_render_command(
    paths: &AppPaths,
    backend_id: &str,
    request_json: &Path,
    manifest_json: &Path,
    report_json: &Path,
    output_dir: &Path,
    item_id: &str,
    track_id: &str,
    variant_label: Option<&str>,
) -> Result<VoiceBackendAdapterResolvedCommand> {
    let detail = get_voice_backend_adapter_detail(paths, backend_id)?;
    let config = detail.config.ok_or_else(|| {
        EngineError::InstallFailed(format!("adapter config not found: {}", detail.template.backend_id))
    })?;
    if !config.enabled {
        return Err(EngineError::InstallFailed(format!(
            "BYO adapter {} is disabled",
            config.backend_id
        )));
    }
    if config.render_command.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "BYO adapter {} is missing render_command tokens",
            config.backend_id
        )));
    }

    let root_dir = config.root_dir.as_deref().map(PathBuf::from);
    if let Some(root_dir) = root_dir.as_ref() {
        if !root_dir.is_dir() {
            return Err(EngineError::InstallFailed(format!(
                "BYO adapter {} root_dir is missing: {}",
                config.backend_id,
                root_dir.display()
            )));
        }
    }
    if let Some(model_dir) = config.model_dir.as_deref() {
        let model_dir = model_dir.trim();
        let model_dir_exists = if model_dir.is_empty() {
            true
        } else {
            let model_path = PathBuf::from(model_dir);
            model_path.exists()
                || root_dir
                    .as_ref()
                    .map(|root| root.join(model_dir).exists())
                    .unwrap_or(false)
        };
        if !model_dir_exists {
            return Err(EngineError::InstallFailed(format!(
                "BYO adapter {} model_dir is missing: {}",
                config.backend_id, model_dir
            )));
        }
    }

    let program_token = config
        .render_command
        .first()
        .ok_or_else(|| EngineError::InstallFailed("render_command is empty".to_string()))?;
    let program = resolve_program_token(
        &expand_render_token(
            program_token,
            root_dir.as_deref(),
            config.python_exe.as_deref(),
            config.model_dir.as_deref(),
            request_json,
            manifest_json,
            report_json,
            output_dir,
            &config.backend_id,
            item_id,
            track_id,
            variant_label,
        ),
        root_dir.as_deref(),
    );
    let program_string = program.to_string_lossy().trim().to_string();
    if program_string.is_empty() {
        return Err(EngineError::InstallFailed(format!(
            "BYO adapter {} render command resolved to an empty program",
            config.backend_id
        )));
    }

    let args = config
        .render_command
        .iter()
        .skip(1)
        .map(|token| {
            expand_render_token(
                token,
                root_dir.as_deref(),
                config.python_exe.as_deref(),
                config.model_dir.as_deref(),
                request_json,
                manifest_json,
                report_json,
                output_dir,
                &config.backend_id,
                item_id,
                track_id,
                variant_label,
            )
        })
        .collect::<Vec<_>>();

    Ok(VoiceBackendAdapterResolvedCommand {
        program: program_string,
        args,
        current_dir: root_dir.map(|value| value.to_string_lossy().to_string()),
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
    expand_token_pairs(
        token,
        &[
            ("{python_exe}", python_exe.unwrap_or("python").to_string()),
            (
                "{root_dir}",
                root_dir
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ),
            ("{model_dir}", model_dir.unwrap_or_default().to_string()),
        ],
    )
}

fn expand_render_token(
    token: &str,
    root_dir: Option<&Path>,
    python_exe: Option<&str>,
    model_dir: Option<&str>,
    request_json: &Path,
    manifest_json: &Path,
    report_json: &Path,
    output_dir: &Path,
    backend_id: &str,
    item_id: &str,
    track_id: &str,
    variant_label: Option<&str>,
) -> String {
    expand_token_pairs(
        token,
        &[
            ("{python_exe}", python_exe.unwrap_or("python").to_string()),
            (
                "{root_dir}",
                root_dir
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ),
            ("{model_dir}", model_dir.unwrap_or_default().to_string()),
            (
                "{request_json}",
                request_json.to_string_lossy().to_string(),
            ),
            (
                "{manifest_json}",
                manifest_json.to_string_lossy().to_string(),
            ),
            (
                "{report_json}",
                report_json.to_string_lossy().to_string(),
            ),
            (
                "{output_dir}",
                output_dir.to_string_lossy().to_string(),
            ),
            ("{backend_id}", backend_id.to_string()),
            ("{item_id}", item_id.to_string()),
            ("{track_id}", track_id.to_string()),
            (
                "{variant_label}",
                variant_label.unwrap_or_default().to_string(),
            ),
        ],
    )
}

fn expand_token_pairs(token: &str, replacements: &[(&str, String)]) -> String {
    let mut out = token.to_string();
    for (marker, value) in replacements {
        out = out.replace(marker, value);
    }
    out
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
                render_command: vec![],
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
                render_command: vec![],
                notes: None,
                updated_at_ms: 0,
            },
        )
        .expect("upsert");
        let detail = probe_voice_backend_adapter(&paths, "cosyvoice").expect("probe");
        assert_eq!(detail.last_probe.as_ref().map(|value| value.ready), Some(true));
    }

    #[test]
    fn resolve_render_command_expands_placeholders() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        upsert_voice_backend_adapter(
            &paths,
            VoiceBackendAdapterConfig {
                backend_id: "cosyvoice".to_string(),
                enabled: true,
                root_dir: Some(dir.path().to_string_lossy().to_string()),
                python_exe: Some("python3".to_string()),
                model_dir: Some(dir.path().join("models").to_string_lossy().to_string()),
                entry_command: vec![],
                probe_command: vec![],
                render_command: vec![
                    "{python_exe}".to_string(),
                    "run_adapter.py".to_string(),
                    "--request".to_string(),
                    "{request_json}".to_string(),
                    "--manifest".to_string(),
                    "{manifest_json}".to_string(),
                    "--report".to_string(),
                    "{report_json}".to_string(),
                    "--out".to_string(),
                    "{output_dir}".to_string(),
                    "--backend".to_string(),
                    "{backend_id}".to_string(),
                    "--item".to_string(),
                    "{item_id}".to_string(),
                    "--track".to_string(),
                    "{track_id}".to_string(),
                    "--variant".to_string(),
                    "{variant_label}".to_string(),
                ],
                notes: None,
                updated_at_ms: 0,
            },
        )
        .expect("upsert");
        std::fs::create_dir_all(dir.path().join("models")).expect("models");
        let resolved = resolve_voice_backend_adapter_render_command(
            &paths,
            "cosyvoice",
            &dir.path().join("request.json"),
            &dir.path().join("manifest.json"),
            &dir.path().join("report.json"),
            &dir.path().join("render_out"),
            "item-1",
            "track-1",
            Some("alt_a"),
        )
        .expect("resolve");
        assert_eq!(resolved.program, "python3");
        assert!(resolved.args.iter().any(|value| value.ends_with("request.json")));
        assert!(resolved.args.iter().any(|value| value.ends_with("manifest.json")));
        assert!(resolved.args.iter().any(|value| value.ends_with("report.json")));
        assert!(resolved.args.iter().any(|value| value.ends_with("render_out")));
        assert!(resolved.args.iter().any(|value| value == "cosyvoice"));
        assert!(resolved.args.iter().any(|value| value == "item-1"));
        assert!(resolved.args.iter().any(|value| value == "track-1"));
        assert!(resolved.args.iter().any(|value| value == "alt_a"));
        assert_eq!(
            resolved.current_dir.as_deref(),
            Some(dir.path().to_string_lossy().as_ref())
        );
    }

    #[test]
    fn starter_recipes_exist_for_research_backends() {
        let templates = adapter_templates();
        for backend_id in ["cosyvoice", "seed_vc", "xtts_v2"] {
            let template = templates
                .iter()
                .find(|value| value.backend_id == backend_id)
                .expect("template");
            assert!(
                !template.starter_recipes.is_empty(),
                "starter recipes missing for {backend_id}"
            );
        }
    }

    #[test]
    fn apply_starter_recipe_populates_commands_and_preserves_paths() {
        let updated = apply_voice_backend_starter_recipe(
            VoiceBackendAdapterConfig {
                backend_id: "cosyvoice".to_string(),
                enabled: false,
                root_dir: Some("D:/voice/cosyvoice".to_string()),
                python_exe: Some("D:/Python/python.exe".to_string()),
                model_dir: None,
                entry_command: vec![],
                probe_command: vec![],
                render_command: vec![],
                notes: None,
                updated_at_ms: 0,
            },
            "cosyvoice_python_wrapper",
        )
        .expect("apply");
        assert!(updated.enabled);
        assert_eq!(updated.root_dir.as_deref(), Some("D:/voice/cosyvoice"));
        assert_eq!(updated.python_exe.as_deref(), Some("D:/Python/python.exe"));
        assert_eq!(updated.model_dir.as_deref(), Some("pretrained_models"));
        assert_eq!(updated.entry_command.first().map(String::as_str), Some("{python_exe}"));
        assert_eq!(updated.probe_command.first().map(String::as_str), Some("{python_exe}"));
        assert_eq!(updated.render_command.first().map(String::as_str), Some("{python_exe}"));
        assert!(
            updated
                .notes
                .as_deref()
                .unwrap_or("")
                .contains("Starter recipe")
        );
    }
}
