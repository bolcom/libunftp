# Changelog

## 2021-03-26 libunftp v0.17.0

_tag: libunftp-0.17.0_


The main focus of this release was the removal of contained authentication and storage back-ends from the libunftp crate and into their own crates. As you can imagine this brings about breaking changes. To see the impack of these

Source code for these crates can still be found in this repository under the `crates` directory.

Breaking Changes:

- Split the GCS back-end into crate `unftp-sbe-gcs`
- Split the Filesystem back-end into crate `unftp-sbe-fs`
- Split the JSON file authenticator into crate `unftp-auth-jsonfile`
- Split the PAM authenticator into crate `unftp-auth-pam`
- Split the REST authenticator into crate `unftp-auth-rest`
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
