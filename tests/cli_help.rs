//! Black-box tests for the public command-line surface.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_exposes_the_public_command_surface() {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("lanyard-ssh-agent"));
    command.arg("--help");

    command.assert().success().stdout(
        predicate::str::contains("serve")
            .and(predicate::str::contains("register"))
            .and(predicate::str::contains("unregister"))
            .and(predicate::str::contains("status"))
            .and(predicate::str::contains("socket")),
    );
}
