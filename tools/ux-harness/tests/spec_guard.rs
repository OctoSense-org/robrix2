//! Exercise the repository's actual CI shell entry point with an isolated CLI fixture.
use std::path::PathBuf;
use std::process::Command;

fn run_regressions(args: &[&str]) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg(root.join("scripts/test_spec_guard.py"))
        .args(args)
        .output()
        .expect("Python 3 is required by the spec guard regression tests");
    assert!(output.status.success(), "{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn spec_guard_composition_regressions() {
    run_regressions(&[]);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(12))]
    #[test]
    fn prop_spec_guard_composition_keeps_all_owners(count in 2usize..7, unowned in proptest::bool::ANY) {
        run_regressions(&["--generated", &count.to_string(), if unowned { "1" } else { "0" }]);
    }
}
