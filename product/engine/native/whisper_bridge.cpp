#include "whisper.h"

#include <cctype>
#include <cstring>
#include <cstdlib>
#include <string>

static thread_local std::string g_last_error;

static void set_error(const std::string &message) { g_last_error = message; }

static std::string trim_copy(const char *s) {
    if (!s) {
        return {};
    }

    const char *start = s;
    while (*start && std::isspace(static_cast<unsigned char>(*start))) {
        start++;
    }

    const char *end = start + std::strlen(start);
    while (end > start && std::isspace(static_cast<unsigned char>(end[-1]))) {
        end--;
    }

    return std::string(start, static_cast<size_t>(end - start));
}

static std::string json_escape(const std::string &in) {
    std::string out;
    out.reserve(in.size() + 16);
    for (char c : in) {
        switch (c) {
        case '\\': out += "\\\\"; break;
        case '"': out += "\\\""; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (static_cast<unsigned char>(c) < 0x20) {
                // Drop other control chars.
            } else {
                out.push_back(c);
            }
        }
    }
    return out;
}

extern "C" {

const char *ytf_whisper_last_error() { return g_last_error.c_str(); }

void ytf_whisper_free_string(char *s) { std::free(s); }

char *ytf_whisper_transcribe_json(const char *model_path,
                                 const float *samples,
                                 int n_samples,
                                 const char *language,
                                 int n_threads,
                                 bool translate) {
    g_last_error.clear();

    if (!model_path || !samples || n_samples <= 0) {
        set_error("invalid arguments");
        return nullptr;
    }

    whisper_context_params cparams = whisper_context_default_params();
    cparams.use_gpu = false;
    cparams.flash_attn = false;
    cparams.gpu_device = -1;

    whisper_context *ctx = whisper_init_from_file_with_params(model_path, cparams);
    if (!ctx) {
        set_error("failed to init whisper context");
        return nullptr;
    }

    whisper_full_params wparams = whisper_full_default_params(WHISPER_SAMPLING_GREEDY);
    wparams.n_threads = n_threads > 0 ? n_threads : 1;
    wparams.translate = translate;
    wparams.print_special = false;
    wparams.print_progress = false;
    wparams.print_realtime = false;
    wparams.print_timestamps = false;
    wparams.token_timestamps = false;
    wparams.no_timestamps = false;

    if (!language || language[0] == '\0' || std::strcmp(language, "auto") == 0) {
        wparams.language = "auto";
        wparams.detect_language = true;
    } else {
        wparams.language = language;
        wparams.detect_language = false;
    }

    const int rc = whisper_full(ctx, wparams, samples, n_samples);
    if (rc != 0) {
        whisper_free(ctx);
        set_error("whisper_full failed");
        return nullptr;
    }

    const int lang_id = whisper_full_lang_id(ctx);
    const char *detected_lang = whisper_lang_str(lang_id);

    const int n_segments = whisper_full_n_segments(ctx);
    std::string json;
    json.reserve(static_cast<size_t>(n_segments) * 128 + 64);
    json += "{";
    json += "\"lang\":\"";
    json += json_escape(detected_lang ? detected_lang : "");
    json += "\",\"segments\":[";

    for (int i = 0; i < n_segments; i++) {
        const int64_t t0 = whisper_full_get_segment_t0(ctx, i);
        const int64_t t1 = whisper_full_get_segment_t1(ctx, i);
        const char *text_raw = whisper_full_get_segment_text(ctx, i);

        const std::string text = trim_copy(text_raw);
        if (text.empty()) {
            continue;
        }

        if (json.back() != '[') {
            json += ",";
        }

        json += "{";
        json += "\"start_ms\":";
        json += std::to_string(t0 * 10);
        json += ",\"end_ms\":";
        json += std::to_string(t1 * 10);
        json += ",\"text\":\"";
        json += json_escape(text);
        json += "\"}";
    }

    json += "]}";
    whisper_free(ctx);

    char *out = static_cast<char *>(std::malloc(json.size() + 1));
    if (!out) {
        set_error("malloc failed");
        return nullptr;
    }
    std::memcpy(out, json.data(), json.size());
    out[json.size()] = '\0';
    return out;
}

} // extern "C"
