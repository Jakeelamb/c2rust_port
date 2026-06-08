use std::process::Command;

fn fixture_root(name: &str) -> camino::Utf8PathBuf {
    let root = camino::Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .unwrap()
        .join(format!("c2rust-port-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn inferred_target(source: &camino::Utf8Path) -> camino::Utf8PathBuf {
    source
        .parent()
        .unwrap()
        .join(format!("{}-rs", source.file_name().unwrap()))
}

#[test]
fn single_command_maps_makefile_repo_without_compile_commands() {
    let root = fixture_root("make");
    std::fs::write(root.join("math_ops.h"), "int add(int lhs, int rhs);\n").unwrap();
    std::fs::write(
        root.join("math_ops.c"),
        "#include \"math_ops.h\"\n\nint add(int lhs, int rhs) {\n    return lhs + rhs;\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("main.c"),
        "#include \"math_ops.h\"\n\nint main(void) {\n    return add(2, 3);\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Makefile"),
        "all:\n\tcc main.c math_ops.c -o main\n",
    )
    .unwrap();
    let target = inferred_target(&root);
    std::fs::create_dir_all(target.join("src")).unwrap();
    std::fs::write(
        target.join("Cargo.toml"),
        "[package]\nname = \"fixture-rs\"\nedition = \"2024\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        target.join("src/lib.rs"),
        "pub fn add(lhs: i32, rhs: i32) -> i32 {\n    lhs + rhs\n}\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        root.join(".c2rust-port/inspect/source-inventory.json")
            .exists()
    );
    assert!(
        root.join(".c2rust-port/inspect/diagnostic-runs.jsonl")
            .exists()
    );
    assert!(
        inferred_target(&root)
            .join(".c-to-rust-port/units/000-source-map/TASK.md")
            .exists()
    );
    assert!(
        inferred_target(&root)
            .join(".c-to-rust-port/CITATION_VALIDATION.json")
            .exists()
    );
    assert!(
        inferred_target(&root)
            .join(".c-to-rust-port/CITATION_VALIDATION.md")
            .exists()
    );
    assert!(root.join(".c2rust-port/knowledge/repo-map.json").exists());
    assert!(root.join(".c2rust-port/knowledge/evidence.db").exists());
    assert!(
        root.join(".c2rust-port/knowledge/capability-matrix.json")
            .exists()
    );
    assert!(
        root.join(".c2rust-port/knowledge/EVIDENCE_QUERIES.md")
            .exists()
    );
    let repo_map =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/repo-map.md")).unwrap();
    assert!(repo_map.contains("## Process Flow"));
    assert!(repo_map.contains("## Data Flow"));
    assert!(repo_map.contains("add"));
    let symbols =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/symbols.jsonl")).unwrap();
    let calls = std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/call_edges.jsonl"))
        .unwrap();
    let build_units =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/build_units.jsonl"))
            .unwrap();
    let repo_map_facts =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/repo_map.jsonl")).unwrap();
    let benchmarks =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/benchmarks.jsonl"))
            .unwrap();
    let types =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/types.jsonl")).unwrap();
    let dataflow =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/dataflow_edges.jsonl"))
            .unwrap();
    let equivalence_diffs =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/equivalence_diffs.jsonl"))
            .unwrap();
    let capabilities =
        std::fs::read_to_string(root.join(".c2rust-port/knowledge/facts/capabilities.jsonl"))
            .unwrap();
    assert!(symbols.contains("\"name\":\"add\""));
    assert!(calls.contains("\"callee\":\"add\""));
    assert!(build_units.contains("math_ops.c"));
    assert!(repo_map_facts.contains("rust_mirror_module"));
    assert!(benchmarks.contains("benchmark_manifest"));
    assert!(benchmarks.contains("benchmark_run"));
    assert!(types.is_empty() || types.contains("\"fact_type\":\"type\""));
    assert!(dataflow.contains("function:add"));
    assert!(equivalence_diffs.contains("\"cpp_entity\":\"add\""));
    assert!(capabilities.contains("\"fact_type\":\"capability\""));
    let mirror =
        std::fs::read_to_string(target.join(".c-to-rust-port/RUST_MIRROR_PLAN.md")).unwrap();
    assert!(mirror.contains("src/source/math_ops.rs"));
}

#[test]
fn single_command_maps_cmake_repo_with_compile_commands() {
    let root = fixture_root("cmake");
    std::fs::write(root.join("main.c"), "int main(void) { return 0; }\n").unwrap();
    std::fs::write(
        root.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.20)\nproject(fixture C)\nadd_executable(fixture main.c)\n",
    )
    .unwrap();
    std::fs::write(root.join("compile_commands.json"), "[]\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let build =
        std::fs::read_to_string(root.join(".c2rust-port/inspect/build-system.json")).unwrap();
    assert!(build.contains("\"has_cmake\": true"));
    assert!(build.contains("\"has_compile_commands\": true"));
}

#[test]
fn single_command_packets_include_cpp_headers_and_restrictions() {
    let source = fixture_root("cpp-source");
    std::fs::write(
        source.join("widget.hpp"),
        "template <class T> T id(T value) { return value; }\n",
    )
    .unwrap();
    std::fs::write(source.join("widget.cpp"), "#include \"widget.hpp\"\n").unwrap();
    std::fs::write(
        source.join("bt2_idx.cpp"),
        "int build_index(void) { return 0; }\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let target = inferred_target(&source);
    let task = std::fs::read_to_string(target.join(".c-to-rust-port/units/000-source-map/TASK.md"))
        .unwrap();
    assert!(task.contains("bt2_idx.cpp"));
    assert!(task.contains("`indexing` source slice"));
    assert!(task.contains("Do not run Cargo"));
    assert!(task.contains("SOURCE_REPO_MAP.md"));
    assert!(task.contains("RUST_MIRROR_PLAN.md"));
    assert!(task.contains("EVIDENCE_QUERIES.md"));
    assert!(task.contains("CITATION_VALIDATION.md"));
    assert!(task.contains("evidence.db"));
    assert!(task.contains("Required Source Evidence"));
    assert!(task.contains("Seed Citations"));
    assert!(task.contains("facts/<table>.jsonl:<key>"));
    assert!(task.contains("Patch Contract"));
    assert!(task.contains("Do not mark an existing file as `new file mode`"));
    let translator =
        std::fs::read_to_string(target.join(".c-to-rust-port/agents/01-translator.md")).unwrap();
    let source_reviewer = std::fs::read_to_string(
        target.join(".c-to-rust-port/agents/02-source-fidelity-reviewer.md"),
    )
    .unwrap();
    let rust_reviewer =
        std::fs::read_to_string(target.join(".c-to-rust-port/agents/03-rust-reviewer.md")).unwrap();
    let review_checklist =
        std::fs::read_to_string(target.join(".c-to-rust-port/agents/REVIEW_CHECKLIST.md")).unwrap();
    let runbook =
        std::fs::read_to_string(target.join(".c-to-rust-port/agents/RUNBOOK.md")).unwrap();
    let citation_validation =
        std::fs::read_to_string(target.join(".c-to-rust-port/CITATION_VALIDATION.json")).unwrap();
    let profile = std::fs::read_to_string(
        target.join(".c-to-rust-port/prompt_profiles/translator-default.toml"),
    )
    .unwrap();
    assert!(translator.contains("Do not run commands"));
    assert!(translator.contains("Source Evidence"));
    assert!(source_reviewer.contains("cited knowledge rows"));
    assert!(source_reviewer.contains("Reject drafts without `Source Evidence` citations"));
    assert!(rust_reviewer.contains("Review a translator draft for Rust correctness"));
    assert!(review_checklist.contains("Existing files are not marked as new files"));
    assert!(review_checklist.contains("fact-row citations"));
    assert!(runbook.contains("Codex roles"));
    assert!(runbook.contains("evidence.db"));
    assert!(citation_validation.contains("\"status\": \"ok\""));
    assert!(profile.contains("require_review_before_apply = true"));
    assert!(profile.contains("translator_forbidden_commands"));
}

#[test]
fn single_command_detects_vendored_source_layout() {
    let root = fixture_root("vendored");
    let source = root.join("spades-rs/reference/upstream/SPAdes-4.2.0");
    let target = root.join("spades-rs");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        target.join("Cargo.toml"),
        "[package]\nname = \"spades-rs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(target.join("src")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        target
            .join(".c-to-rust-port/units/000-source-map/TASK.md")
            .exists()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("VendoredSource"));
}
