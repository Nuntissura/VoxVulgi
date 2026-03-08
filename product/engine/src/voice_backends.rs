use crate::{paths::AppPaths, tools};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceBackendCatalogEntry {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub mode: String,
    pub install_mode: String,
    pub status: String,
    pub status_detail: String,
    pub managed_default: bool,
    pub language_scope: String,
    pub reference_expectation: String,
    pub gpu_recommended: bool,
    pub code_license: String,
    pub weights_license: String,
    pub strengths: Vec<String>,
    pub risks: Vec<String>,
    pub primary_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceBackendCatalog {
    pub default_backend_id: String,
    pub performance_tier: String,
    pub backends: Vec<VoiceBackendCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceBackendRecommendationRequest {
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    pub reference_count: Option<usize>,
    pub goal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceBackendRecommendation {
    pub goal: String,
    pub source_lang: String,
    pub target_lang: String,
    pub reference_count: usize,
    pub performance_tier: String,
    pub preferred_backend_id: String,
    pub fallback_backend_id: Option<String>,
    pub rationale: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn backend_catalog(paths: &AppPaths) -> VoiceBackendCatalog {
    let performance = tools::performance_tier_status(paths);
    let pack = tools::tts_voice_preserving_local_v1_pack_status(paths);
    let tier = performance.tier.clone();
    let cosy_status = if let Some(version) = pack.cosyvoice_version.clone() {
        (
            "detected_python_env".to_string(),
            format!("CosyVoice detected in the current Python environment ({version})."),
        )
    } else {
        (
            "available_via_byo".to_string(),
            "Experimental candidate. Not managed by VoxVulgi yet.".to_string(),
        )
    };

    let openvoice_status = if pack.installed {
        (
            "managed_ready".to_string(),
            "Managed VoxVulgi backend is installed and ready.".to_string(),
        )
    } else {
        (
            "managed_missing_pack".to_string(),
            "Managed VoxVulgi backend exists but the local pack is not installed.".to_string(),
        )
    };

    let backends = vec![
        VoiceBackendCatalogEntry {
            id: "openvoice_v2".to_string(),
            display_name: "OpenVoice V2 + Kokoro".to_string(),
            family: "two_stage_tts_plus_vc".to_string(),
            mode: "tts_plus_voice_conversion".to_string(),
            install_mode: "managed".to_string(),
            status: openvoice_status.0,
            status_detail: openvoice_status.1,
            managed_default: true,
            language_scope: "multilingual dubbing with explicit EN text control".to_string(),
            reference_expectation: "1+ short clean references; multi-reference supported".to_string(),
            gpu_recommended: false,
            code_license: "MIT / Apache-2.0 components".to_string(),
            weights_license: "MIT / Apache-2.0 components".to_string(),
            strengths: vec![
                "best fit for subtitle-driven timing control".to_string(),
                "stable current shipped path".to_string(),
                "works well with VoxVulgi's reusable reference library".to_string(),
            ],
            risks: vec![
                "less expressive than newer direct zero-shot TTS systems".to_string(),
                "prosody remains limited by the base TTS stage".to_string(),
            ],
            primary_source: "https://github.com/myshell-ai/OpenVoice".to_string(),
        },
        VoiceBackendCatalogEntry {
            id: "cosyvoice".to_string(),
            display_name: "CosyVoice".to_string(),
            family: "direct_zero_shot_tts".to_string(),
            mode: "direct_conditioned_tts".to_string(),
            install_mode: "byo".to_string(),
            status: cosy_status.0,
            status_detail: cosy_status.1,
            managed_default: false,
            language_scope: "multilingual zero-shot TTS".to_string(),
            reference_expectation: "short clean references; stronger results with higher-quality inputs"
                .to_string(),
            gpu_recommended: true,
            code_license: "Apache-2.0".to_string(),
            weights_license: "Apache-2.0 (check chosen release)".to_string(),
            strengths: vec![
                "strong multilingual zero-shot TTS direction".to_string(),
                "promptable and instruction-friendly".to_string(),
                "good candidate for more expressive speech".to_string(),
            ],
            risks: vec![
                "heavier runtime and packaging cost than the shipped backend".to_string(),
                "still experimental for VoxVulgi integration".to_string(),
            ],
            primary_source: "https://github.com/FunAudioLLM/CosyVoice".to_string(),
        },
        VoiceBackendCatalogEntry {
            id: "seed_vc".to_string(),
            display_name: "Seed-VC".to_string(),
            family: "voice_conversion_only".to_string(),
            mode: "voice_conversion_stage".to_string(),
            install_mode: "byo".to_string(),
            status: "available_via_byo".to_string(),
            status_detail: "Promising conversion-stage candidate, but not managed by VoxVulgi yet."
                .to_string(),
            managed_default: false,
            language_scope: "speech-to-speech timbre transfer".to_string(),
            reference_expectation: "multiple clean references recommended for strong identity transfer"
                .to_string(),
            gpu_recommended: true,
            code_license: "Apache-2.0".to_string(),
            weights_license: "Check chosen release".to_string(),
            strengths: vec![
                "strong candidate for identity-focused timbre transfer".to_string(),
                "good fit as an alternative conversion stage after English TTS".to_string(),
            ],
            risks: vec![
                "does not solve text generation by itself".to_string(),
                "needs explicit adapter integration before use inside VoxVulgi".to_string(),
            ],
            primary_source: "https://github.com/Plachtaa/seed-vc".to_string(),
        },
        VoiceBackendCatalogEntry {
            id: "indextts2".to_string(),
            display_name: "IndexTTS2".to_string(),
            family: "direct_zero_shot_tts".to_string(),
            mode: "direct_conditioned_tts".to_string(),
            install_mode: "byo".to_string(),
            status: "available_via_byo".to_string(),
            status_detail: "Research-backed direct TTS candidate; not managed by VoxVulgi yet."
                .to_string(),
            managed_default: false,
            language_scope: "zero-shot TTS with duration and emotion control".to_string(),
            reference_expectation: "short clean references; quality scales with better voice samples"
                .to_string(),
            gpu_recommended: true,
            code_license: "MIT".to_string(),
            weights_license: "Check chosen release".to_string(),
            strengths: vec![
                "duration and emotion control map well to dubbing".to_string(),
                "promising similarity profile for voice cloning".to_string(),
            ],
            risks: vec![
                "newer project with less packaging proof inside VoxVulgi".to_string(),
                "should be benchmarked before promotion".to_string(),
            ],
            primary_source: "https://github.com/index-tts/index-tts".to_string(),
        },
        VoiceBackendCatalogEntry {
            id: "fish_speech".to_string(),
            display_name: "Fish-Speech".to_string(),
            family: "direct_zero_shot_tts".to_string(),
            mode: "direct_conditioned_tts".to_string(),
            install_mode: "byo".to_string(),
            status: "available_via_byo".to_string(),
            status_detail: "Long-form expressive candidate; not managed by VoxVulgi yet.".to_string(),
            managed_default: false,
            language_scope: "multilingual long-form expressive TTS".to_string(),
            reference_expectation: "clean references and GPU-tier hardware recommended".to_string(),
            gpu_recommended: true,
            code_license: "Apache-2.0".to_string(),
            weights_license: "Check chosen release".to_string(),
            strengths: vec![
                "strong expressive research direction".to_string(),
                "valuable long-form comparator".to_string(),
            ],
            risks: vec![
                "heavier stack than the current managed path".to_string(),
                "best treated as an experimental adapter candidate first".to_string(),
            ],
            primary_source: "https://github.com/fishaudio/fish-speech".to_string(),
        },
        VoiceBackendCatalogEntry {
            id: "xtts_v2".to_string(),
            display_name: "XTTS v2".to_string(),
            family: "direct_zero_shot_tts".to_string(),
            mode: "direct_conditioned_tts".to_string(),
            install_mode: "byo".to_string(),
            status: "available_via_byo".to_string(),
            status_detail: "Practical OSS reference backend; not managed by VoxVulgi yet.".to_string(),
            managed_default: false,
            language_scope: "multilingual zero-shot TTS".to_string(),
            reference_expectation: "clean short references".to_string(),
            gpu_recommended: true,
            code_license: "MPL-2.0".to_string(),
            weights_license: "Check chosen release".to_string(),
            strengths: vec![
                "mature OSS surface".to_string(),
                "useful direct-TTS baseline for comparisons".to_string(),
            ],
            risks: vec![
                "not obviously better than newer candidates without benchmarking".to_string(),
            ],
            primary_source: "https://github.com/coqui-ai/TTS".to_string(),
        },
    ];

    VoiceBackendCatalog {
        default_backend_id: "openvoice_v2".to_string(),
        performance_tier: tier,
        backends,
    }
}

pub fn recommend_backend(
    paths: &AppPaths,
    request: VoiceBackendRecommendationRequest,
) -> VoiceBackendRecommendation {
    let catalog = backend_catalog(paths);
    recommend_backend_for_catalog(&catalog, request)
}

fn recommend_backend_for_catalog(
    catalog: &VoiceBackendCatalog,
    request: VoiceBackendRecommendationRequest,
) -> VoiceBackendRecommendation {
    let goal = normalize_goal(request.goal.as_deref());
    let source_lang = normalize_lang(request.source_lang.as_deref()).unwrap_or_else(|| "auto".to_string());
    let target_lang = normalize_lang(request.target_lang.as_deref()).unwrap_or_else(|| "en".to_string());
    let reference_count = request.reference_count.unwrap_or(0);
    let tier = catalog.performance_tier.clone();

    let mut rationale = vec![format!("Performance tier: {tier}.")];
    if source_lang == "ja" || source_lang == "ko" {
        rationale.push(format!(
            "Current item is oriented around {source_lang} -> {target_lang} dubbing."
        ));
    } else {
        rationale.push(format!(
            "Recommendation is being generated for {source_lang} -> {target_lang} dubbing."
        ));
    }
    rationale.push(format!("Reference clips available: {reference_count}."));

    let (preferred_backend_id, fallback_backend_id) = match goal.as_str() {
        "identity" if tier == "gpu" && reference_count >= 2 => {
            rationale.push(
                "Identity-first goal with multiple references favors a stronger experimental conversion-stage candidate."
                    .to_string(),
            );
            ("seed_vc".to_string(), Some("openvoice_v2".to_string()))
        }
        "expressive" if tier == "gpu" => {
            rationale.push(
                "Expressive goal on a GPU-tier machine favors a direct zero-shot TTS candidate."
                    .to_string(),
            );
            ("cosyvoice".to_string(), Some("openvoice_v2".to_string()))
        }
        "timing" | "speed" => {
            rationale.push(
                "Timing- or speed-first work should stay on the managed subtitle-driven backend."
                    .to_string(),
            );
            ("openvoice_v2".to_string(), None)
        }
        _ => {
            rationale.push(
                "Balanced production work should stay on the current managed backend until a benchmark report supports a switch."
                    .to_string(),
            );
            ("openvoice_v2".to_string(), None)
        }
    };

    let mut warnings: Vec<String> = Vec::new();
    if preferred_backend_id != "openvoice_v2" {
        warnings.push(
            "Preferred backend is experimental/BYO; keep OpenVoice as the safe production fallback until the benchmark lab proves otherwise."
                .to_string(),
        );
    }
    if reference_count == 0 {
        warnings.push(
            "No speaker references are configured yet; clone quality will be limited regardless of backend."
                .to_string(),
        );
    } else if reference_count == 1 {
        warnings.push(
            "Only one reference is available; multi-reference profiles are more reliable for identity-focused comparisons."
                .to_string(),
        );
    }
    if tier != "gpu" && preferred_backend_id != "openvoice_v2" {
        warnings.push(
            "The current machine is not in the GPU tier; experimental direct-TTS or VC candidates will be slower and may be impractical."
                .to_string(),
        );
    }

    VoiceBackendRecommendation {
        goal,
        source_lang,
        target_lang,
        reference_count,
        performance_tier: tier,
        preferred_backend_id,
        fallback_backend_id,
        rationale,
        warnings,
    }
}

fn normalize_lang(value: Option<&str>) -> Option<String> {
    let raw = value?.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    match raw.as_str() {
        "jpn" => Some("ja".to_string()),
        "kor" => Some("ko".to_string()),
        "eng" => Some("en".to_string()),
        _ => Some(raw),
    }
}

fn normalize_goal(value: Option<&str>) -> String {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if matches!(v.as_str(), "identity" | "expressive" | "timing" | "speed") => v,
        _ => "balanced".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_catalog(tier: &str) -> VoiceBackendCatalog {
        VoiceBackendCatalog {
            default_backend_id: "openvoice_v2".to_string(),
            performance_tier: tier.to_string(),
            backends: vec![],
        }
    }

    #[test]
    fn balanced_goal_prefers_openvoice() {
        let rec = recommend_backend_for_catalog(
            &test_catalog("gpu"),
            VoiceBackendRecommendationRequest {
                source_lang: Some("ko".to_string()),
                target_lang: Some("en".to_string()),
                reference_count: Some(3),
                goal: Some("balanced".to_string()),
            },
        );
        assert_eq!(rec.preferred_backend_id, "openvoice_v2");
        assert!(rec.fallback_backend_id.is_none());
    }

    #[test]
    fn identity_goal_prefers_seed_vc_with_gpu_and_multi_ref() {
        let rec = recommend_backend_for_catalog(
            &test_catalog("gpu"),
            VoiceBackendRecommendationRequest {
                source_lang: Some("ja".to_string()),
                target_lang: Some("en".to_string()),
                reference_count: Some(3),
                goal: Some("identity".to_string()),
            },
        );
        assert_eq!(rec.preferred_backend_id, "seed_vc");
        assert_eq!(rec.fallback_backend_id.as_deref(), Some("openvoice_v2"));
    }

    #[test]
    fn expressive_goal_on_cpu_falls_back_to_openvoice() {
        let rec = recommend_backend_for_catalog(
            &test_catalog("cpu"),
            VoiceBackendRecommendationRequest {
                source_lang: Some("ko".to_string()),
                target_lang: Some("en".to_string()),
                reference_count: Some(2),
                goal: Some("expressive".to_string()),
            },
        );
        assert_eq!(rec.preferred_backend_id, "openvoice_v2");
    }
}
