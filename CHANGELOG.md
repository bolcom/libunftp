# Changelog

## 2021-04-18 libunftp v0.17.1

_tag: libunftp-0.17.1_

Changes in this release:

- [#327](https://github.com/bolcom/libunftp/issues/327) Allow PROT and PBSZ without requiring authentication.
- [#330](https://github.com/bolcom/libunftp/pull/330) Load TLS certificates only once at startup instead of on every connect.

## 2021-03-26 Newly splitted auth and storage back-ends

- Released [unftp-sbe-gcs](https://crates.io/crates/unftp-sbe-gcs)
- Released [unftp-sbe-fs](https://crates.io/crates/unftp-sbe-fs)
- Released [unftp-auth-jsonfile](https://crates.io/crates/unftp-auth-jsonfile)
- Released [unftp-auth-pam](https://crates.io/crates/unftp-auth-pam)
- Released [unftp-auth-rest](https://crates.io/crates/unftp-auth-rest)

## 2021-03-26 libunftp v0.17.0

_tag: libunftp-0.17.0_


The main focus of this release was the removal of contained authentication and storage back-ends from the libunftp crate 
and into their own crates. As you can imagine this brings about breaking changes.

Source code for these crates can still be found in this repository under the `crates` directory.

Breaking Changes:

- Split the GCS back-end into crate [unftp-sbe-gcs](https://crates.io/crates/unftp-sbe-gcs)
- Split the Filesystem back-end into crate [unftp-sbe-fs](https://crates.io/crates/unftp-sbe-fs)
- Split the JSON file authenticator into crate [unftp-auth-jsonfile](https://crates.io/crates/unftp-auth-jsonfile)
- Split the PAM authenticator into crate [unftp-auth-pam](https://crates.io/crates/unftp-auth-pam)
- Split the REST authenticator into crate [unftp-auth-rest](https://crates.io/crates/unftp-auth-rest)
- Changed some public API names to adhere to Rust naming conventions:
  - PAMAuthenticator became PamAuthenticator
  - PassiveHost::IP became PassiveHost::Ip
  - PassiveHost::DNS became PassiveHost::Dns
  - RestError::HTTPStatusError became RestError::HttpStatusError
  - RestError::JSONDeserializationError became RestError::JsonDeserializationError
  - RestError::JSONSerializationError became RestError::JsonSerializationError
- The `Server::with_fs` method moved into the `ServerExt` extension trait of `unftp-sbe-fs`
- The `Server::with_fs_and_auth` method was removed. Use the `Server::with_authenticator` method instead.


Other changes:

- Upgraded outdated dependencies
