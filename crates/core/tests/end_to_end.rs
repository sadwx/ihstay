//! End-to-end integration tests that exercise core pieces together.
//!
//! These tests don't boot the Tauri app; they verify the data flow:
//! board.jsonl on disk → parser → store → compaction round-trip.

use chrono::{Duration, Utc};
use ihstay_core::board::{compaction, parser, store::StateStore};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn add_line(session_id: &str, ts: &str, kind: &str) -> String {
    format!(
        r#"{{"op":"add","ts":"{ts}","session_id":"{session_id}","cwd":"/tmp","claude_pid":1,"terminal_pid":null,"transcript_path":"/tmp/t","notification_type":"{kind}","message":"m"}}"#
    )
}

fn clear_line(session_id: &str, ts: &str) -> String {
    format!(r#"{{"op":"clear","ts":"{ts}","session_id":"{session_id}","reason":"user_replied"}}"#)
}

fn stale_line(session_id: &str, ts: &str) -> String {
    format!(r#"{{"op":"stale","ts":"{ts}","session_id":"{session_id}","reason":"pid_dead"}}"#)
}

#[test]
fn golden_path_replay_and_snapshot_order() {
    let text = [
        add_line("a", "2026-04-17T10:00:00Z", "permission_prompt"),
        add_line("b", "2026-04-17T10:01:00Z", "idle_prompt"),
        add_line("c", "2026-04-17T10:02:00Z", "permission_prompt"),
        clear_line("a", "2026-04-17T10:03:00Z"),
    ]
    .join("\n");

    let (ops, skipped) = parser::parse_lines(&text);
    assert_eq!(ops.len(), 4);
    assert_eq!(skipped, 0);

    let mut store = StateStore::new();
    store.apply_all(ops);

    let snapshot = store.snapshot();
    assert_eq!(snapshot.len(), 2);
    // c (permission, ts 10:02) before b (idle, ts 10:01)
    assert_eq!(snapshot[0].session_id, "c");
    assert_eq!(snapshot[1].session_id, "b");
}

#[test]
fn compaction_roundtrip_preserves_current_state() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("board.jsonl");

    let text = [
        add_line("a", "2026-04-17T10:00:00Z", "permission_prompt"),
        add_line("b", "2026-04-17T10:01:00Z", "idle_prompt"),
        clear_line("a", "2026-04-17T10:03:00Z"),
        add_line("d", "2026-04-17T10:04:00Z", "permission_prompt"),
    ]
    .join("\n")
        + "\n";

    fs::write(&path, text).unwrap();

    let result = compaction::compact(&path, Duration::hours(24)).unwrap();
    assert_eq!(result.entries_before, 4);
    assert_eq!(result.entries_after, 2);

    let content = fs::read_to_string(&path).unwrap();
    let (ops, _) = parser::parse_lines(&content);
    let mut store = StateStore::new();
    store.apply_all(ops);
    let snapshot = store.snapshot();
    let ids: Vec<&str> = snapshot.iter().map(|e| e.session_id.as_str()).collect();
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"d"));
    assert!(!ids.contains(&"a"));
}

#[test]
fn expired_stale_entries_dropped_during_compaction() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("board.jsonl");

    let old = (Utc::now() - Duration::hours(48)).to_rfc3339();
    let fresh = (Utc::now() - Duration::hours(1)).to_rfc3339();
    let stale_old = (Utc::now() - Duration::hours(25)).to_rfc3339();
    let stale_fresh = Utc::now().to_rfc3339();

    let text = [
        add_line("expired", &old, "permission_prompt"),
        stale_line("expired", &stale_old),
        add_line("recent", &fresh, "permission_prompt"),
        stale_line("recent", &stale_fresh),
    ]
    .join("\n")
        + "\n";

    fs::write(&path, text).unwrap();

    compaction::compact(&path, Duration::hours(24)).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("recent"));
    assert!(!content.contains("expired"));
}

#[test]
fn unknown_op_ignored_for_forward_compat() {
    let text = concat!(
        r#"{"op":"add","ts":"2026-04-17T10:00:00Z","session_id":"a","cwd":"/tmp","claude_pid":1,"terminal_pid":null,"transcript_path":"/tmp/t","notification_type":"permission_prompt","message":"m"}"#,
        "\n",
        r#"{"op":"future_op","ts":"2026-04-17T10:01:00Z","session_id":"a","reason":"x"}"#,
        "\n",
    );
    let (ops, skipped) = parser::parse_lines(text);
    assert_eq!(ops.len(), 1);
    assert_eq!(skipped, 1);
}
