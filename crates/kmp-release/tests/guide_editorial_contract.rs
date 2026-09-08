use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::application::use_cases::load_guide_editorial::LoadGuideEditorial;
use kmp_release::domain::repository_root::RepositoryRoot;
use serde_json::{Value, json};

fn load(entry: Value, file_text: &str) -> Result<String, String> {
    let directory = tempfile::tempdir().expect("test directory");
    let guide = directory.path().join("plugins/kmp/guide");
    std::fs::create_dir_all(guide.join("examples")).expect("guide directory");
    std::fs::write(guide.join("examples/history.md"), file_text).expect("example");
    std::fs::write(
        guide.join("editorial.json"),
        json!({
            "schema_version": 1, "guide_version": "1", "observed_at": "2026-09-01T00:00:00Z",
            "abouts": [{
                "about": "guide:kmp-agent", "audience": "agent", "anchor": "history",
                "entries": [entry], "relations": []
            }]
        })
        .to_string(),
    )
    .expect("editorial");
    let root = RepositoryRoot::from_path(directory.path()).expect("repository");
    LoadGuideEditorial::new(&SystemFileSystem)
        .execute(&root)
        .map(|source| source.abouts[0].entries[0].text.clone())
        .map_err(|error| error.to_string())
}

fn entry() -> Value {
    json!({"id": "history", "kind": "instruction", "depth": "advanced", "evidence": "example source"})
}

#[test]
fn keeps_existing_inline_text_and_file_backed_examples_exact() {
    let body = "# Cambio de decisión\n\n```json\n{\"why\": \"requisito explícito\"}\n```\n";
    let mut inline = entry();
    inline["text"] = body.into();
    assert_eq!(load(inline, "unused").expect("inline"), body);
    let mut file = entry();
    file["text_file"] = "examples/history.md".into();
    assert_eq!(load(file, body).expect("file"), body);
}

#[test]
fn rejects_ambiguous_missing_and_empty_bodies() {
    let mut both = entry();
    both["text"] = "inline".into();
    both["text_file"] = "examples/history.md".into();
    assert!(
        load(both, "file")
            .expect_err("invalid editorial must fail")
            .contains("either inline text")
    );
    assert!(
        load(entry(), "unused")
            .expect_err("invalid editorial must fail")
            .contains("has no text")
    );
    let mut empty = entry();
    empty["text_file"] = "examples/history.md".into();
    assert!(
        load(empty, " \n")
            .expect_err("invalid editorial must fail")
            .contains("has no text")
    );
    let mut missing = entry();
    missing["text_file"] = "examples/missing.md".into();
    assert!(
        load(missing, "unused")
            .expect_err("invalid editorial must fail")
            .contains("missing.md")
    );
}

#[test]
fn rejects_absolute_and_parent_paths() {
    for path in [
        "/etc/passwd",
        "../outside.md",
        "examples/../../outside.md",
        "",
    ] {
        let mut file = entry();
        file["text_file"] = path.into();
        assert!(
            load(file, "unused")
                .expect_err("invalid editorial must fail")
                .contains("relative text_file")
        );
    }
}
