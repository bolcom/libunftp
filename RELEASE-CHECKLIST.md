# Release Checklist

* Update minor versions dependencies
* Update Cargo.toml with the new version number
* Search for the old version number to find references to it in documentation and update those occurrences.
* Run `make pr-prep`, ensuring everything is green
* Before releasing libunftp itself, run unFTP while pointing to the new version of libunftp
* Make a new commit indicating the crate name and version number e.g.    
    > Release libunftp version x.y.x

    or

    > Release unftp-sbe-fs version x.y.x
* Prepare release notes for the Github release page
* Run `make publish`
* Push to Github
* Create the release in Github using tag format {component}-{version} e.g.
  > libunftp-0.17.1
  or
  > unftp-sbe-fs-0.1.1    
* Notify the Telegram channel.
