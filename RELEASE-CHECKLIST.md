# Release Checklist

* Update minor versions dependencies. Install [cargo-edit](https://crates.io/crates/cargo-edit) and run `cargo upgrade`.
  Cargo-edit also covers all the crates in the workspace
  You can also use `cargo upgrade --dry-run` to just check what is outstanding or this oneliner:
  `cat Cargo.toml | sed -n '33,56p' | awk '{ print $1 }' | xargs -L1 cargo search --limit=1`
* Review changes since each affected crate's previous release tag before choosing versions,
  including public APIs, FTP protocol behavior, dependency requirements, and supported toolchains/platforms.
  For pre-1.0 crates (`0.y.z`, `y > 0`), increment `z` for backwards-compatible changes,
  including additive features, and `y` for breaking changes, following Cargo's compatibility convention.
* Update Cargo.toml with the new version number (including `unftp-core` and affected dependants when releasing core APIs)
* Search for the old version number to find references to it in documentation and update those occurrences.
  Do this for all the crates that will be released
* Run `make pr-prep`, ensuring everything is green, and the CI-equivalent checks and feature
  combinations documented in AGENTS.md. Handle Docker-dependent GCS tests separately as documented there.
* For FTP pathname parsing changes, verify real control/data-channel regression tests and preservation
  of leading, embedded, and trailing whitespace, including paths following LIST options and `--`.
* Before releasing libunftp itself, run unFTP while pointing to the new version of libunftp
* Update CHANGELOG.md for all the crates affected. Link entries to associated merged pull requests
  where available using `[#123](https://github.com/bolcom/libunftp/pull/123)`; verify associations
  against GitHub metadata or commit history.
* Prepare release notes for the GitHub release page
* Make a new commit (don't push) indicating the crate name and version number e.g.
  > Release libunftp version x.y.x

  or

  > Release unftp-sbe-fs version x.y.x

  or

  > Release unftp-core version x.y.x

  or

  > Release all
* Run `make publish`
* Push to GitHub
* Create the release in GitHub using tag format {component}-{version} e.g.
  > libunftp-0.17.1
  or
  > unftp-sbe-fs-0.1.1

  or

  > unftp-core-0.1.0
* Notify the Telegram channel.
