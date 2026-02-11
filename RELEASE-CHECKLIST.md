# Release Checklist

* Update minor versions dependencies. Install [cargo-edit](https://crates.io/crates/cargo-edit) and run `cargo upgrade`.
  Cargo-edit also covers all the crates in the workspace
  You can also use `cargo upgrade --dry-run` to just check what is outstanding or this oneliner:
  `cat Cargo.toml | sed -n '33,56p' | awk '{ print $1 }' | xargs -L1 cargo search --limit=1`
* Update Cargo.toml with the new version number (including `unftp-core` when releasing core APIs)
* Search for the old version number to find references to it in documentation and update those occurrences.
  Do this for all the crates that will be released
* Run `make pr-prep`, ensuring everything is green
* Before releasing libunftp itself, run unFTP while pointing to the new version of libunftp
* Update CHANGELOG.md for all the crates affected
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
