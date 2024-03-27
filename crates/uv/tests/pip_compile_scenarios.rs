//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/sync.sh
//! Scenarios from <https://github.com/zanieb/packse/tree/0.3.12/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]

use std::env;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};
use predicates::prelude::predicate;

use common::{create_bin_with_executables, get_bin, uv_snapshot, TestContext, INSTA_FILTERS};

mod common;

/// Provision python binaries and return a `pip compile` command with options shared across all scenarios.
fn command(context: &TestContext, python_versions: &[&str]) -> Command {
    let bin = create_bin_with_executables(&context.temp_dir, python_versions)
        .expect("Failed to create bin dir");
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("compile")
        .arg("requirements.in")
        .arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.12/simple-html/")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/zanieb/packse/0.3.12/vendor/links.html")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (8 * 1024 * 1024).to_string());
    }

    command
}

/// The user requires a package which requires a Python version greater than the
/// current version, but they use an alternative Python version for package
/// resolution.
///
/// ```text
/// incompatible-python-compatible-override
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn incompatible_python_compatible_override() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"incompatible-python-compatible-override-", "package-"));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("incompatible-python-compatible-override-a==1.0.0")?;

    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: true
                 exit_code: 0
                 ----- stdout -----
                 # This file was autogenerated by uv via the following command:
                 #    uv pip compile requirements.in --cache-dir [CACHE_DIR] --python-version=3.11
                 package-a==1.0.0

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                 Resolved 1 package in [TIME]
                 "###
    );

    output.assert().success().stdout(predicate::str::contains(
        "incompatible-python-compatible-override-a==1.0.0",
    ));

    Ok(())
}

/// The user requires a package which requires a compatible Python version, but they
/// request an incompatible Python version for package resolution.
///
/// ```text
/// compatible-python-incompatible-override
/// ├── environment
/// │   └── python3.11
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10
/// ```
#[test]
fn compatible_python_incompatible_override() -> Result<()> {
    let context = TestContext::new("3.11");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"compatible-python-incompatible-override-", "package-"));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("compatible-python-incompatible-override-a==1.0.0")?;

    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.9")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.9 is not available; 3.11.7 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the requested Python version (3.9) does not satisfy Python>=3.10 and package-a==1.0.0 depends on Python>=3.10, we can conclude that package-a==1.0.0 cannot be used.
                       And because you require package-a==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There are only
/// source distributions available for the package.
///
/// ```text
/// incompatible-python-compatible-override-unavailable-no-wheels
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn incompatible_python_compatible_override_unavailable_no_wheels() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"incompatible-python-compatible-override-unavailable-no-wheels-",
        "package-",
    ));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in
        .write_str("incompatible-python-compatible-override-unavailable-no-wheels-a==1.0.0")?;

    // Since there are no wheels for the package and it is not compatible with the
    // local installation, we cannot build the source distribution to determine its
    // dependencies.
    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and package-a==1.0.0 depends on Python>=3.10, we can conclude that package-a==1.0.0 cannot be used.
                       And because you require package-a==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There are only
/// source distributions available for the package. The user has a compatible Python
/// version installed elsewhere on their system.
///
/// ```text
/// incompatible-python-compatible-override-available-no-wheels
/// ├── environment
/// │   ├── python3.11
/// │   └── python3.9 (active)
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn incompatible_python_compatible_override_available_no_wheels() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &["3.11"];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"incompatible-python-compatible-override-available-no-wheels-",
        "package-",
    ));
    // test

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in
        .write_str("incompatible-python-compatible-override-available-no-wheels-a==1.0.0")?;

    // Since there is a compatible Python version available on the system, it should be
    // used to build the source distributions.
    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: true
                 exit_code: 0
                 ----- stdout -----
                 # This file was autogenerated by uv via the following command:
                 #    uv pip compile requirements.in --cache-dir [CACHE_DIR] --python-version=3.11
                 package-a==1.0.0

                 ----- stderr -----
                 Resolved 1 package in [TIME]
                 "###
    );

    output.assert().success().stdout(predicate::str::contains(
        "incompatible-python-compatible-override-available-no-wheels-a==1.0.0",
    ));

    Ok(())
}

/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There is a
/// wheel available for the package, but it does not have a compatible tag.
///
/// ```text
/// incompatible-python-compatible-override-no-compatible-wheels
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn incompatible_python_compatible_override_no_compatible_wheels() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"incompatible-python-compatible-override-no-compatible-wheels-",
        "package-",
    ));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in
        .write_str("incompatible-python-compatible-override-no-compatible-wheels-a==1.0.0")?;

    // Since there are no compatible wheels for the package and it is not compatible
    // with the local installation, we cannot build the source distribution to
    // determine its dependencies.
    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and package-a==1.0.0 depends on Python>=3.10, we can conclude that package-a==1.0.0 cannot be used.
                       And because you require package-a==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There are only
/// source distributions available for the compatible version of the package, but
/// there is an incompatible version with a wheel available.
///
/// ```text
/// incompatible-python-compatible-override-other-wheel
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a
/// │       ├── satisfied by a-1.0.0
/// │       └── satisfied by a-2.0.0
/// └── a
///     ├── a-1.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     └── a-2.0.0
///         └── requires python>=3.12 (incompatible with environment)
/// ```
#[test]
fn incompatible_python_compatible_override_other_wheel() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"incompatible-python-compatible-override-other-wheel-",
        "package-",
    ));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("incompatible-python-compatible-override-other-wheel-a")?;

    // Since there are no wheels for the version of the package compatible with the
    // target and it is not compatible with the local installation, we cannot build the
    // source distribution to determine its dependencies. The other version has wheels
    // available, but is not compatible with the target version and cannot be used.
    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and package-a==1.0.0 depends on Python>=3.10, we can conclude that package-a==1.0.0 cannot be used.
                       And because only the following versions of package-a are available:
                           package-a==1.0.0
                           package-a==2.0.0
                       we can conclude that package-a<2.0.0 cannot be used. (1)

                       Because the requested Python version (3.11) does not satisfy Python>=3.12 and package-a==2.0.0 depends on Python>=3.12, we can conclude that package-a==2.0.0 cannot be used.
                       And because we know from (1) that package-a<2.0.0 cannot be used, we can conclude that all versions of package-a cannot be used.
                       And because you require package-a, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// The user requires a package which requires a Python version with a patch version
/// and the user provides a target version without a patch version.
///
/// ```text
/// python-patch-override-no-patch
/// ├── environment
/// │   └── python3.8.18
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8.4
/// ```
#[test]
fn python_patch_override_no_patch() -> Result<()> {
    let context = TestContext::new("3.8.18");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"python-patch-override-no-patch-", "package-"));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("python-patch-override-no-patch-a==1.0.0")?;

    // Since the resolver is asked to solve with 3.8, the minimum compatible Python
    // requirement is treated as 3.8.0.
    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.8")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the requested Python version (3.8) does not satisfy Python>=3.8.4 and package-a==1.0.0 depends on Python>=3.8.4, we can conclude that package-a==1.0.0 cannot be used.
                       And because you require package-a==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// The user requires a package which requires a Python version with a patch version
/// and the user provides a target version with a compatible patch version.
///
/// ```text
/// python-patch-override-patch-compatible
/// ├── environment
/// │   └── python3.8.18
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8.0
/// ```
#[test]
fn python_patch_override_patch_compatible() -> Result<()> {
    let context = TestContext::new("3.8.18");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"python-patch-override-patch-compatible-", "package-"));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("python-patch-override-patch-compatible-a==1.0.0")?;

    let output = uv_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.8.0")
        , @r###"
                 success: true
                 exit_code: 0
                 ----- stdout -----
                 # This file was autogenerated by uv via the following command:
                 #    uv pip compile requirements.in --cache-dir [CACHE_DIR] --python-version=3.8.0
                 package-a==1.0.0

                 ----- stderr -----
                 warning: The requested Python version 3.8.0 is not available; 3.8.18 will be used to build dependencies instead.
                 Resolved 1 package in [TIME]
                 "###
    );

    output.assert().success().stdout(predicate::str::contains(
        "python-patch-override-patch-compatible-a==1.0.0",
    ));

    Ok(())
}
