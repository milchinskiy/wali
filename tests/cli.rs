#![cfg(unix)]

mod common;

use common::*;

fn combined_output(output: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[test]
fn root_command_reports_expected_commands_instead_of_placeholder() {
    let output = run_wali_failure(&[]);
    let combined = combined_output(&output);

    assert!(combined.contains("expected command"), "root command error should explain the problem: {combined}");
    assert!(combined.contains("plan"), "root command error should mention plan: {combined}");
    assert!(combined.contains("check"), "root command error should mention check: {combined}");
    assert!(combined.contains("apply"), "root command error should mention apply: {combined}");
    assert!(
        !combined.contains("not implemented"),
        "root command must not expose an implementation placeholder: {combined}"
    );
}

#[test]
fn verbosity_options_are_not_dead_accepted_flags() {
    for args in [["-v"], ["--verbosity"]] {
        let output = run_wali_failure(&args);
        let combined = combined_output(&output);

        assert!(
            !combined.contains("not implemented"),
            "removed verbosity option {args:?} should fail during argument parsing, not root handling: {combined}"
        );
    }
}
