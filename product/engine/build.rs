use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let whisper_root = manifest_dir.join("third_party").join("whisper.cpp");

    let bridge = manifest_dir.join("native").join("whisper_bridge.cpp");

    println!("cargo:rerun-if-changed={}", bridge.display());
    println!(
        "cargo:rerun-if-changed={}",
        whisper_root.join("include/whisper.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        whisper_root.join("src/whisper.cpp").display()
    );

    // ggml base sources
    let ggml_src = whisper_root.join("ggml").join("src");
    let ggml_cpu = ggml_src.join("ggml-cpu");

    let include_whisper = whisper_root.join("include");
    let include_ggml = whisper_root.join("ggml").join("include");
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| String::new());
    let mut cpu_arch_sources = Vec::new();
    if target_arch == "x86_64" || target_arch == "x86" {
        cpu_arch_sources.push(ggml_cpu.join("arch\\x86\\quants.c"));
        cpu_arch_sources.push(ggml_cpu.join("arch\\x86\\repack.cpp"));
    } else if target_arch == "aarch64" || target_arch == "arm" {
        cpu_arch_sources.push(ggml_cpu.join("arch\\arm\\quants.c"));
        cpu_arch_sources.push(ggml_cpu.join("arch\\arm\\repack.cpp"));
    } else if target_arch == "riscv64" {
        cpu_arch_sources.push(ggml_cpu.join("arch\\riscv\\quants.c"));
    } else if target_arch == "powerpc64" || target_arch == "powerpc" {
        cpu_arch_sources.push(ggml_cpu.join("arch\\powerpc\\quants.c"));
    } else if target_arch == "s390x" {
        cpu_arch_sources.push(ggml_cpu.join("arch\\s390\\quants.c"));
    } else if target_arch == "loongarch64" {
        cpu_arch_sources.push(ggml_cpu.join("arch\\loongarch\\quants.c"));
    }

    // Build C sources as C.
    let mut build_c = cc::Build::new();
    build_c
        .warnings(false)
        .include(&include_whisper)
        .include(&include_ggml)
        .include(ggml_src.clone())
        .include(ggml_cpu.clone())
        .define("GGML_USE_CPU", None)
        .define("GGML_VERSION", Some("\"0.0.0\""))
        .define("GGML_COMMIT", Some("\"unknown\""))
        .file(ggml_src.join("ggml.c"))
        .file(ggml_src.join("ggml-alloc.c"))
        .file(ggml_src.join("ggml-quants.c"))
        .file(ggml_cpu.join("ggml-cpu.c"))
        .file(ggml_cpu.join("quants.c"));

    if build_c.get_compiler().is_like_msvc() {
        build_c.define("_CRT_SECURE_NO_WARNINGS", None);
    } else {
        build_c.flag_if_supported("-std=c11");
    }

    build_c.compile("ytf_whisper_c");

    // Build C++ sources as C++17.
    let mut build_cpp = cc::Build::new();
    build_cpp
        .warnings(false)
        .cpp(true)
        .include(&include_whisper)
        .include(&include_ggml)
        .include(ggml_src.clone())
        .include(ggml_cpu.clone())
        .define("GGML_USE_CPU", None)
        .define("GGML_VERSION", Some("\"0.0.0\""))
        .define("GGML_COMMIT", Some("\"unknown\""))
        .define("WHISPER_VERSION", Some("\"0.0.0\""))
        .file(bridge)
        .file(whisper_root.join("src").join("whisper.cpp"))
        .files(
            cpu_arch_sources
                .iter()
                .map(|path| path.as_path())
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .file(ggml_src.join("ggml.cpp"))
        .file(ggml_src.join("ggml-backend.cpp"))
        .file(ggml_src.join("ggml-backend-dl.cpp"))
        .file(ggml_src.join("ggml-backend-reg.cpp"))
        .file(ggml_src.join("ggml-opt.cpp"))
        .file(ggml_src.join("ggml-threading.cpp"))
        .file(ggml_src.join("gguf.cpp"))
        .file(ggml_cpu.join("ggml-cpu.cpp"))
        .file(ggml_cpu.join("repack.cpp"))
        .file(ggml_cpu.join("hbm.cpp"))
        .file(ggml_cpu.join("traits.cpp"))
        .file(ggml_cpu.join("amx").join("amx.cpp"))
        .file(ggml_cpu.join("amx").join("mmq.cpp"))
        .file(ggml_cpu.join("binary-ops.cpp"))
        .file(ggml_cpu.join("unary-ops.cpp"))
        .file(ggml_cpu.join("vec.cpp"))
        .file(ggml_cpu.join("ops.cpp"));

    if build_cpp.get_compiler().is_like_msvc() {
        build_cpp.flag_if_supported("/std:c++17");
        build_cpp.flag_if_supported("/utf-8");
        build_cpp.define("_CRT_SECURE_NO_WARNINGS", None);
    } else {
        build_cpp.flag_if_supported("-std=c++17");
    }

    build_cpp.compile("ytf_whisper_cpp");
}
