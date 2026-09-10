//! Contract tests: the published specs vs. the crates that implement them
//! (MOD-TEST-CONTRACT).
//!
//! Scope of this first slice (audit 2026-09-10, theme B — `tests/contract/`
//! was an empty shell): the terminal request family, whose schema bounds the
//! Shell previously did not enforce at all.
//!
//! The gate is deliberately *implementation-first*: instead of re-reading the
//! schema with a second validator, every assertion drives the real wire types
//! (`dsh_daemon::terminal::*`) with fixture-shaped input. `scripts/validate-specs.mjs`
//! already owns schema <-> fixture agreement; what it cannot see is whether the
//! implementation agrees with either.

use std::path::{Path, PathBuf};

use dsh_daemon::terminal::{TerminalCreateRequest, TerminalResizeRequest, TerminalWriteRequest};
use dsh_terminal_provider::{MAX_COLS, MAX_ROWS, MAX_WRITE_BYTES, MIN_COLS, MIN_ROWS};

/// Repository root, derived from this crate's manifest location
/// (`<root>/tests/contract/Cargo.toml`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

fn fixture(name: &str) -> String {
    let path = repo_root().join("specs/terminal/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn schema(name: &str) -> serde_json::Value {
    let path = repo_root().join("specs/terminal").join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn parameter(document: &serde_json::Value, property: &str, key: &str) -> u64 {
    document["properties"][property][key]
        .as_u64()
        .unwrap_or_else(|| panic!("schema is missing properties.{property}.{key}"))
}

/// The geometry bounds are declared in the schemas and enforced by the
/// provider; they must be the same numbers. This test fails the moment either
/// side drifts — the exact class of drift the audit found ("schema as
/// documentation").
#[test]
fn terminal_geometry_bounds_match_the_schemas_and_the_provider() {
    let create = schema("terminal-create-request.schema.json");
    let resize = schema("terminal-resize-request.schema.json");

    for document in [&create, &resize] {
        assert_eq!(parameter(document, "cols", "minimum") as u16, MIN_COLS);
        assert_eq!(parameter(document, "cols", "maximum") as u16, MAX_COLS);
        assert_eq!(parameter(document, "rows", "minimum") as u16, MIN_ROWS);
        assert_eq!(parameter(document, "rows", "maximum") as u16, MAX_ROWS);
    }

    // The write bound is bytes on the provider side and characters in the
    // schema; ASCII fixtures keep the two comparable.
    let write = schema("terminal-write-request.schema.json");
    assert_eq!(
        parameter(&write, "data", "maxLength") as usize,
        MAX_WRITE_BYTES
    );
    assert_eq!(parameter(&write, "data", "minLength"), 1);

    // The create schema shells enum is the cross-platform union the provider
    // narrows per platform.
    let shells: Vec<&str> = create["properties"]["shell"]["enum"]
        .as_array()
        .expect("shell enum")
        .iter()
        .map(|value| value.as_str().expect("shell value"))
        .collect();
    assert_eq!(
        shells,
        vec!["default", "cmd", "powershell", "pwsh", "sh", "bash", "zsh"]
    );
    let cwd_max = parameter(&create, "cwd", "maxLength");
    assert_eq!(cwd_max, 1024);
}

/// The daemon's create wire type accepts every `.valid.` fixture of the
/// create schema — including the shell variants — and rejects the invalid
/// ones. A fixture the implementation cannot parse is a contract break even
/// when the schema itself is happy.
#[test]
fn terminal_create_wire_type_agrees_with_every_fixture() {
    for name in [
        "terminal-create-request.valid.json",
        "terminal-create-request.shell-bash.valid.json",
        "terminal-create-request.shell-zsh.valid.json",
        "terminal-create-request.agent-automation.valid.json",
    ] {
        let text = fixture(name);
        let request: TerminalCreateRequest =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} must deserialize: {e}"));
        assert!(request.is_valid(), "{name} must be a valid create request");
    }

    // `mode` outside the schema enum: the type parses (it is a String) but
    // the request is rejected — this is the local fail-closed gate the Shell
    // proxy and the daemon handler both rely on.
    let invalid: TerminalCreateRequest =
        serde_json::from_str(&fixture("terminal-create-request.invalid.json"))
            .expect("wire shape parses");
    assert!(!invalid.is_valid(), "an unknown mode must never be valid");

    // A create that carries agent facts in human mode is cross-mode, i.e.
    // fail-closed (schema `allOf` + `then.agent: false`).
    let cross_mode: TerminalCreateRequest = serde_json::from_str(&fixture(
        "terminal-create-request.agent-automation.valid.json",
    ))
    .expect("wire shape parses");
    assert!(cross_mode.is_valid(), "agent_automation carries its facts");
    let mut without_agent = cross_mode;
    without_agent.agent = None;
    assert!(
        !without_agent.is_valid(),
        "agent_automation without agent facts must be rejected"
    );
}

/// Unknown fields are rejected by the wire types (`deny_unknown_fields`), which
/// is what keeps a spec addition from silently flowing through an old Shell.
#[test]
fn terminal_wire_types_reject_unknown_and_out_of_shape_fields() {
    let base = fixture("terminal-create-request.valid.json");
    let mut value: serde_json::Value = serde_json::from_str(&base).expect("fixture parses");
    value["surprise"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<TerminalCreateRequest>(value).is_err(),
        "unknown create field must be rejected"
    );

    // A negative or oversized geometry is not a `u16`: the wire shape itself
    // rejects it before any handler runs.
    for bad in [serde_json::json!(-1), serde_json::json!(70_000)] {
        let mut value: serde_json::Value = serde_json::from_str(&base).expect("fixture parses");
        value["cols"] = bad.clone();
        assert!(
            serde_json::from_value::<TerminalCreateRequest>(value).is_err(),
            "cols {bad} must not deserialize"
        );
    }

    // The write and resize families keep the same shape guarantees.
    let write: TerminalWriteRequest =
        serde_json::from_str(&fixture("terminal-write-request.valid.json"))
            .expect("write fixture deserializes");
    assert!(!write.data.is_empty());
    assert!(write.session_id.starts_with("pty-"));
    let resize: TerminalResizeRequest =
        serde_json::from_str(&fixture("terminal-resize-request.valid.json"))
            .expect("resize fixture deserializes");
    assert!((MIN_COLS..=MAX_COLS).contains(&resize.cols));
    assert!((MIN_ROWS..=MAX_ROWS).contains(&resize.rows));
}

/// Every schema in `specs/terminal/` must exist for every wire type this crate
/// exercises: a renamed or deleted schema must break the build, not just the
/// documentation.
#[test]
fn terminal_schemas_exist_for_the_exercised_wire_types() {
    for name in [
        "terminal-create-request.schema.json",
        "terminal-write-request.schema.json",
        "terminal-resize-request.schema.json",
        "terminal-close-request.schema.json",
        "terminal-report.schema.json",
        "terminal-output-event.schema.json",
    ] {
        let document = schema(name);
        assert!(
            document["$id"]
                .as_str()
                .is_some_and(|id| id.ends_with(name)),
            "{name} must declare a matching $id"
        );
        assert!(
            document["properties"].is_object(),
            "{name} declares properties"
        );
    }
}
