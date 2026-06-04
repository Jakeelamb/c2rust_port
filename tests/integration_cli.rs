use std::process::Command;

fn fixture_root(name: &str) -> camino::Utf8PathBuf {
    let root = camino::Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .unwrap()
        .join(format!("c2rust-port-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn inspect_makefile_repo_without_compile_commands() {
    let root = fixture_root("make");
    std::fs::write(root.join("main.c"), "int main(void) { return 0; }\n").unwrap();
    std::fs::write(root.join("Makefile"), "all:\n\tcc main.c -o main\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg("inspect")
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
}

#[test]
fn inspect_cmake_repo_with_compile_commands() {
    let root = fixture_root("cmake");
    std::fs::write(root.join("main.c"), "int main(void) { return 0; }\n").unwrap();
    std::fs::write(
        root.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.20)\nproject(fixture C)\nadd_executable(fixture main.c)\n",
    )
    .unwrap();
    std::fs::write(root.join("compile_commands.json"), "[]\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg("inspect")
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
fn packets_include_cpp_headers_and_restrictions() {
    let source = fixture_root("cpp-source");
    let target = fixture_root("cpp-target");
    std::fs::create_dir_all(target.join("src")).unwrap();
    std::fs::write(
        source.join("widget.hpp"),
        "template <class T> T id(T value) { return value; }\n",
    )
    .unwrap();
    std::fs::write(source.join("widget.cpp"), "#include \"widget.hpp\"\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg("packets")
        .arg(&source)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let task = std::fs::read_to_string(target.join(".c-to-rust-port/units/000-source-map/TASK.md"))
        .unwrap();
    assert!(task.contains("widget.hpp"));
    assert!(task.contains("Do not run Cargo"));
}

#[test]
fn init_vendored_source_layout_is_dry_by_default() {
    let root = fixture_root("vendored");
    let source = root.join("spades-rs/reference/upstream/SPAdes-4.2.0");
    let target = root.join("spades-rs");
    std::fs::create_dir_all(&source).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_c2rust-port"))
        .arg("init")
        .arg("--source")
        .arg(&source)
        .arg("--target")
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!target.join("Cargo.toml").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SPAdes-4.2.0"));
}
