//! Behavioral coverage for the persistent user memory selection (#680).
//!
//! The defect was not a wrong path, it was a silent one: a workspace with no
//! project marker reached the per-user default store and nothing said so.
//! These tests drive the real binary the way a person does — save a
//! selection, restart the process, stand somewhere else — and pin the whole
//! precedence, both refusals, and the promise that an old store is never
//! touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Machine {
    root: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        let machine = Self {
            root: tempfile::tempdir().expect("isolated machine"),
        };
        for relative in ["home", "config", "data", "workspace", "repository/src"] {
            fs::create_dir_all(machine.at(relative)).expect("isolated directory");
        }
        // A repository is a project root; the workspace deliberately is not.
        fs::create_dir_all(machine.at("repository/.git")).expect("project marker");
        machine
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    /// Runs the real binary from `working_dir` on an isolated machine.
    fn run(&self, working_dir: &str, arguments: &[&str]) -> Output {
        self.run_with_override(working_dir, arguments, None)
    }

    fn run_with_override(
        &self,
        working_dir: &str,
        arguments: &[&str],
        data_dir: Option<&Path>,
    ) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
        command
            .args(arguments)
            .current_dir(self.at(working_dir))
            .env("HOME", self.at("home"))
            .env("XDG_CONFIG_HOME", self.at("config"))
            .env("XDG_DATA_HOME", self.at("data"))
            .env_remove("KMP_MCP_DATA_DIR");
        if let Some(data_dir) = data_dir {
            command.env("KMP_MCP_DATA_DIR", data_dir);
        }
        command.output().expect("the binary runs")
    }

    fn config_file(&self) -> PathBuf {
        self.at("config").join("kmp").join("config.toml")
    }

    fn user_default_store(&self) -> PathBuf {
        self.at("data").join("kmp").join("default")
    }

    /// A store stamped for a format this engine will not open, with a file in
    /// it whose bytes must survive everything these tests do.
    fn unopenable_store(&self, relative: &str) -> PathBuf {
        let path = self.at(relative);
        fs::create_dir_all(path.join("store")).expect("store directory");
        fs::write(path.join("FORMAT_VERSION"), "2\n").expect("old stamp");
        fs::write(path.join("store").join("kernel.redb"), b"older memory").expect("old store");
        path
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// One long line again, whatever width the report wrapped it to.
fn unwrapped(report: &str) -> String {
    report.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_says(report: &str, clauses: &[&str]) {
    for clause in clauses {
        assert!(report.contains(clause), "missing `{clause}` in:\n{report}");
    }
}

/// The whole precedence, one rule at a time, through the real binary: the
/// environment override, then the saved selection, then project discovery,
/// then the per-user default.
#[test]
fn the_saved_selection_sits_between_the_environment_and_project_discovery() {
    let machine = Machine::new();
    let chosen = machine.at("chosen-memory");
    let explicit = machine.at("explicit-memory");

    // Nothing saved: a workspace with no project marker reaches the per-user
    // default, and says so.
    let before = machine.run("workspace", &["config"]);
    assert_eq!(before.status.code(), Some(0), "{}", stderr(&before));
    assert_says(
        &stdout(&before),
        &[
            "saved selection: none (automatic selection)",
            &format!(
                "effective memory: {}",
                machine.user_default_store().display()
            ),
            "chosen by: user",
        ],
    );

    // Saving is one command and does not start a host or create a store.
    let saved = machine.run("workspace", &["config", "memory-store", path(&chosen)]);
    assert_eq!(saved.status.code(), Some(0), "{}", stderr(&saved));
    assert_says(
        &stdout(&saved),
        &[
            &format!("saved selection: {}", chosen.display()),
            &format!("effective memory: {}", chosen.display()),
            "chosen by: saved",
            "the selection saved in the user config file",
        ],
    );
    assert!(
        !chosen.exists(),
        "selecting a memory must not create it; the first write does"
    );

    // The selection is a line in the user's config file, so it survives every
    // process restart rather than living in one process's environment.
    let config = fs::read_to_string(machine.config_file()).expect("config file");
    assert_eq!(
        config.trim(),
        format!("memory_store = \"{}\"", chosen.display())
    );

    // A separate process, standing inside a project, still reaches it.
    let in_a_project = machine.run("repository/src", &["config"]);
    assert_says(
        &stdout(&in_a_project),
        &[
            &format!("effective memory: {}", chosen.display()),
            "chosen by: saved",
        ],
    );

    // The environment override stays the most local thing anyone can say.
    let overridden =
        machine.run_with_override("repository/src", &["config"], Some(explicit.as_path()));
    assert_says(
        &stdout(&overridden),
        &[
            &format!("saved selection: {}", chosen.display()),
            &format!("effective memory: {}", explicit.display()),
            "chosen by: env",
            "KMP_MCP_DATA_DIR",
        ],
    );

    // And the doctor answers the question a reader actually holds: the
    // selection exists, it is not what opened here, and this is why.
    let doctor = machine.run_with_override("repository/src", &["doctor"], Some(explicit.as_path()));
    // The doctor wraps its details to the terminal, so the clause is matched
    // on the text rather than on where the wrapping fell.
    assert_says(
        &unwrapped(&stdout(&doctor)),
        &[&format!(
            "saved selection: {} — not in use here, because env won",
            chosen.display()
        )],
    );

    // Clearing it hands project discovery and the per-user default back.
    let cleared = machine.run("repository/src", &["config", "memory-store", "--clear"]);
    assert_eq!(cleared.status.code(), Some(0), "{}", stderr(&cleared));
    assert_says(
        &stdout(&cleared),
        &[
            "saved selection: none (automatic selection)",
            &format!(
                "effective memory: {}",
                machine.at("repository").join(".kernel").display()
            ),
            "chosen by: project",
        ],
    );
    assert_says(
        &stdout(&machine.run("workspace", &["config"])),
        &[&format!(
            "effective memory: {}",
            machine.user_default_store().display()
        )],
    );
}

/// Every surface that explains the selection explains the same order.
#[test]
fn the_precedence_is_stated_wherever_the_selection_is() {
    let machine = Machine::new();
    assert_says(
        &stdout(&machine.run("workspace", &["config"])),
        &[
            "precedence: the KMP_MCP_DATA_DIR environment variable, then the saved selection, \
             then the nearest project root, then the per-user default",
        ],
    );
}

/// A selection that cannot mean one directory on every restart is refused
/// before anything is written.
#[test]
fn an_invalid_selection_is_refused_and_changes_nothing() {
    let machine = Machine::new();
    for (value, clause) in [
        ("relative/memory", "absolute path"),
        ("~/memory", "does not expand shell paths"),
        ("", "needs an absolute directory path"),
    ] {
        let refused = machine.run("workspace", &["config", "memory-store", value]);
        assert_eq!(refused.status.code(), Some(2), "`{value}` must be refused");
        let message = stderr(&refused);
        assert_says(
            &message,
            &[clause, "repair: run `kmp-mcp config memory-store"],
        );
        assert!(
            !machine.config_file().exists(),
            "a refused selection must not write a config file"
        );
    }
}

/// The hard requirement in #680: a store this engine cannot open is refused
/// with a repair, and every byte of it is still there afterwards.
#[test]
fn an_incompatible_store_is_refused_with_a_repair_and_left_untouched() {
    let machine = Machine::new();
    let old = machine.unopenable_store("older-memory");
    let old_store_file = old.join("store").join("kernel.redb");

    let refused = machine.run("workspace", &["config", "memory-store", path(&old)]);
    assert_eq!(refused.status.code(), Some(2), "{}", stdout(&refused));
    let message = stderr(&refused);
    assert_says(
        &message,
        &[
            "cannot select this memory",
            "cannot open",
            "left exactly as it was",
            "nothing was migrated, moved or converted",
            "repair: run `kmp-mcp config memory-store",
            "--clear",
        ],
    );

    assert_eq!(
        fs::read(&old_store_file).expect("the old store is still on disk"),
        b"older memory",
        "a refused selection must never rewrite the store it refused"
    );
    assert_eq!(
        fs::read_to_string(old.join("FORMAT_VERSION")).expect("stamp"),
        "2\n",
        "a refused selection must never restamp the store it refused"
    );
    assert!(
        !machine.config_file().exists(),
        "a refused selection must not be saved"
    );
}

/// Selecting a fresh memory is not a migration: the store that was selected
/// before keeps every byte, and the doctor can still find it.
#[test]
fn selecting_a_new_memory_preserves_the_one_it_replaces() {
    let machine = Machine::new();
    let previous = machine.unopenable_store("data/kmp/default");
    let previous_file = previous.join("store").join("kernel.redb");
    let fresh = machine.at("fresh-memory");
    fs::create_dir_all(&fresh).expect("fresh directory");

    let saved = machine.run("workspace", &["config", "memory-store", path(&fresh)]);
    assert_eq!(saved.status.code(), Some(0), "{}", stderr(&saved));
    assert_says(
        &stdout(&saved),
        &[&format!("effective memory: {}", fresh.display())],
    );

    assert_eq!(
        fs::read(&previous_file).expect("the previous store is still on disk"),
        b"older memory",
        "selecting another memory must not move, convert or delete the old one"
    );
    assert_eq!(
        fs::read_to_string(previous.join("FORMAT_VERSION")).expect("stamp"),
        "2\n"
    );
}

/// A selection saved by hand that no longer parses is reported, never quietly
/// replaced by automatic selection — a silent fall-through is the defect.
#[test]
fn a_broken_saved_selection_is_reported_rather_than_ignored() {
    let machine = Machine::new();
    fs::create_dir_all(machine.config_file().parent().expect("parent")).expect("config dir");
    fs::write(
        machine.config_file(),
        "memory_store = \"relative/memory\"\n",
    )
    .expect("config");

    let broken = machine.run("workspace", &["config"]);
    assert_eq!(broken.status.code(), Some(2), "{}", stdout(&broken));
    assert_says(
        &stderr(&broken),
        &["line 1 has invalid memory_store", "absolute path"],
    );

    // The doctor must fail closed on the same file rather than opening some
    // other memory and calling it healthy.
    let doctor = machine.run("workspace", &["doctor"]);
    assert_eq!(doctor.status.code(), Some(1));
    assert_says(
        &stdout(&doctor),
        &["the saved user memory selection is unusable"],
    );
}

fn path(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}
