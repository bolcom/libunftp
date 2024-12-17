# Changelog

### libunftp 0.20.3

_tag: libunftp-0.20.3_

- [#528](https://github.com/bolcom/libunftp/pull/528) Fix to enable socket reuse that caused 425 errors in passive mode

## 2024-12-15 Release of all crates

### libunftp 0.20.2

_tag: libunftp-0.20.2_

- Upgraded dependencies

### unftp-auth-jsonfile v0.3.5, unftp-auth-pam v0.2.6

- Compiled against libunftp v0.20.2
- Upgraded dependencies

### unftp-auth-rest v0.2.7

- [520](https://github.com/bolcom/libunftp/pull/520) Allow https for auth rest url
- Compiled against libunftp v0.20.2
- Upgraded dependencies

### unftp-sbe-fs v0.2.6, unftp-sbe-gcs v0.2.7

- Compiled against libunftp v0.20.2
- Upgraded dependencies

### libunftp 0.20.1

- Fixed a build issue on Windows
- Upgraded dependencies
- Fixed examples on FreeBSD

### unftp-auth-jsonfile v0.3.4, unftp-auth-pam v0.2.5, unftp-auth-rest v0.2.5

- Compiled against libunftp v0.20.0
- Upgraded dependencies

### unftp-sbe-fs v0.2.5

- Compiled against libunftp v0.20.0
- Fixed the format of the LIST command
- Upgraded dependencies

### unftp-sbe-gcs v0.2.6

- Compiled against libunftp v0.20.0
- Fix listing when root path is set (#509)
- Upgraded dependencies

### libunftp 0.20.0

_tag: libunftp-0.10.0_

- Compile against Rust 1.78.0
- Added support for Capsicum on FreeBSD (#481)
- Fixed proxy protocol issue #494 when removing stale data channel
- Upgraded dependencies
- Code cleanup and documentation improvements
- BREAKING: Introduced a new `ServerBuilder` struct to build the `Server`.

### libunftp 0.19.1

_tag: libunftp-0.19.1_

- Upgraded dependencies

### unftp-auth-rest v0.2.4

- [#492](https://github.com/bolcom/libunftp/pull/492) Added source IP parameter support
- compiled against libunftp v0.19.1

### unftp-auth-jsonfile v0.3.3, unftp-auth-pam v0.2.4

- compiled against libunftp v0.19.1

### unftp-sbe-fs v0.2.4, unftp-auth-pam v0.2.5

- compiled against libunftp v0.19.1

### libunftp 0.19.0

_tag: libunftp-0.19.0_

- [#471](https://github.com/bolcom/libunftp/pull/471) Added unFTP documentation link to help command output
- Include libunftp version in help command output
- [#470](https://github.com/bolcom/libunftp/pull/470) Fixed issue with modified datetime formatting for `Fileinfo` where
  old
  dates didn't render correctly.
- [#482](https://github.com/bolcom/libunftp/pull/482) Fixed RUSTSEC-2023-0052
- Compile against Rust 1.72.0
- BREAKING: Upgrade to latest bitflags dependency. Bitflags are exposed in the API
  for the TlsFlags option.
- Improved tests
- Upgraded dependencies

### unftp-sbe-gcs v0.2.3

- [#449](https://github.com/bolcom/libunftp/pull/449) GCS Backend has had a cleanup (deduplication, modularization)
- [#461](https://github.com/bolcom/libunftp/pull/461) Better GCS error mapping to FTP and convey the causing
- [#465](https://github.com/bolcom/libunftp/pull/465) Handle paginated results for LIST fixing issue #464
- [#466](https://github.com/bolcom/libunftp/pull/465) Fixed an (unreleased) issue regarding root directory affecting
  list and cwd
- [#467](https://github.com/bolcom/libunftp/pull/467) Added more verbose error details for HTTP responses with error
  body
- [#478](https://github.com/bolcom/libunftp/pull/468) Fixed CWD on / error when the directory was empty

### libunftp 0.18.9

_tag: libunftp-0.18.9_

- [#461](https://github.com/bolcom/libunftp/pull/461) Cleaned INFO log output
- [#461](https://github.com/bolcom/libunftp/pull/461) New metrics (ftp_transferred_total, ftp_sent_bytes,
  ftp_received_bytes)
- [#461](https://github.com/bolcom/libunftp/pull/461) Useful new log messages such as data command summary with transfer
  speed
- [#461](https://github.com/bolcom/libunftp/pull/461) Fixed bug where REST command didn't work correctly
- [#461](https://github.com/bolcom/libunftp/pull/461) Various other bug fixes (RETR reply on missing data connection,
  mapping to correct ftp errors)
- [#458](https://github.com/bolcom/libunftp/pull/458), [66756f1](https://github.com/bolcom/libunftp/commit/66756f1af19515c2df65fa58518d7c874fb2497a)
  Added partial support for FTP Active Mode. See `Server::active_passive_mode`
- [#453](https://github.com/bolcom/libunftp/pull/453) Added support for the BYE command
- Upgraded dependencies and Rust version

## 2023-01-25 Release of all crates

### libunftp 0.18.8

_tag: libunftp-0.18.8_

- Upgraded dependencies

### unftp-sbe-gcs v0.2.2

- [#384](https://github.com/bolcom/libunftp/issues/384) Implemented caching of the access token for GCS
- Upgraded dependencies
- compiled against libunftp v0.18.8

### unftp-auth-jsonfile v0.3.1, unftp-auth-{rest,pam} v0.2.2

- Upgraded dependencies
- compiled against libunftp v0.18.8

### 2022-12-07 unftp-auth-jsonfile v0.3.0

- [#441](https://github.com/bolcom/libunftp/issues/441) JsonFile authenticator: support gzipped and base64-encoded file

### 2022-10-26 libunftp 0.18.7

- [#430](https://github.com/bolcom/libunftp/pull/430) Fix issue with proxy protocol hash construction
- [#432](https://github.com/bolcom/libunftp/pull/432) Show Trace ID as hex in debug output
- [#434](https://github.com/bolcom/libunftp/issues/434) Time out if client doesn't connect on data port after PASV
- Upgraded dependencies

### 2022-09-25 libunftp 0.18.6

- [#429](https://github.com/bolcom/libunftp/pull/429) Await proxy protocol header in a separate task, fixes
  issue [#208](https://github.com/bolcom/libunftp/issues/208)
- [#428](https://github.com/bolcom/libunftp/pull/428) Support Elliptic Curve Private Keys
- Upgraded dependencies

## 2022-06-25 Release of all crates

### unftp-auth-gcs v0.2.1

_tag: unftp-auth-jsonfile-0.2.1_

- [#416](https://github.com/bolcom/libunftp/pull/416) GCS support for `RMD`. Plus `CWD` now checks target directory
  existence
- [#415](https://github.com/bolcom/libunftp/pull/415) Support directory timestamps in GCS. To resolve issues with some
  UI FTP clients, such as Cyberduck

### unftp-auth-* v0.2.1

- compiled unftp-auth-pam against libunftp v0.18.5
- compiled unftp-auth-rest against libunftp v0.18.5

### unftp-sbe-* v0.2.1

- compiled unftp-sbe-fs against libunftp v0.18.5
- compiled unftp-sbe-gcs against libunftp v0.18.5

### 2022-06-24 libunftp 0.18.5

_tag: libunftp-0.18.5_

- [#414](https://github.com/bolcom/libunftp/pull/414) Fixed path display issues for Windows clients.
- [#413](https://github.com/bolcom/libunftp/pull/413) Fixed issue where the `OPTS UTF8` command was not handled
  correctly
  as seen with the FTP client included in Windows Explorer.
- Upgraded dependencies

## 2022-01-21 libunftp 0.18.4

_tag: libunftp-0.18.4_

- [#343](https://github.com/bolcom/libunftp/pull/343), anti - brute force password guessing feature, choose from
  different failed login attempts policies: deters
  successive failed login attempts based on IP, username or the combination of both
- [#403](https://github.com/bolcom/libunftp/pull/403), [#404](https://github.com/bolcom/libunftp/pull/404) Improved
  logging: The username and file path are logged in
  separate fields in more places.
- [#405](https://github.com/bolcom/libunftp/pull/405) Improved metrics: The `ftp_reply_total` and `ftp_error_total`
  counters now have new labels `event` and `event_type` to allow correlation with the event for which a reply is given
  or for which an error occurred.
- [#402](https://github.com/bolcom/libunftp/pull/402) Allow `OPTS UTF8 ..` without needing to authenticate.
- Upgraded dependencies

## 2022-01-21 libunftp 0.18.3

_tag: libunftp-0.18.3_

- [#394](https://github.com/bolcom/libunftp/pull/394) Implemented a new API (`Server.notify_data`
  and `Server.notify_presence`)
  to allow listening for file events.
- Upgraded dependencies

## 2021-09-25 libunftp 0.18.2

_tag: libunftp-0.18.2_

- [#386](https://github.com/bolcom/libunftp/issues/386) Implemented graceful shutdown through the
  Server.shutdown_indicator method.
- Upgraded to rustls v0.20.0
- Upgraded other minor dependency versions
- Testing improvements

## 2021-09-25 libunftp 0.18.1

_tag: libunftp-0.18.1_

- Replace futures with futures-util and use Tokio's mpsc channels
- [#371](https://github.com/bolcom/libunftp/pull/371), [#377](https://github.com/bolcom/libunftp/pull/377) Fixed an
  issue where rclone reported all file sizes as 0. The fix was to include the number of links to a file in the output
  to the client.
- Fixed a unit tests
- Upgraded dependencies
- [#379](https://github.com/bolcom/libunftp/pull/379) Fixed an issue where the `Permissions` struct could not be used
  even though it was public.
- [#380](https://github.com/bolcom/libunftp/pull/380), [#381](https://github.com/bolcom/libunftp/pull/381) Return STAT
  response as a multi-line in accordance with RFC 959 in order to fix an issue with the Cyberduck client.

## 2021-07-13 Release of all crates

### libunftp 0.18.0

_tag: libunftp-0.18.0_

- [#356](https://github.com/bolcom/libunftp/pull/356) Authenticators can now also take the connection source IP, and
  the client certificate chain into account in addition to the password when performing authentication.
- [#356](https://github.com/bolcom/libunftp/pull/356/files) **Breaking**: The `Authenticator::authenticate` method now
  takes a `Credentials` structure reference instead of a `str` reference for the second parameter.
- [#373](https://github.com/bolcom/libunftp/pull/373) **Breaking**: The `StorageBackend` methods were all changed to
  take a reference of a user (`&User`) instead of an optional reference to it (`&Option<User>`).
- Dependency upgrades and cleanups
- Fixed an issue where OPTS UTF8 returned the wrong FTP reply code
- [#361](https://github.com/bolcom/libunftp/issues/361) Don't allow consecutive PASS commands
- Added support for TLS client certificates
- [#358](https://github.com/bolcom/libunftp/pull/358/files) Added the ability for authenticators to do password-less
  authentication when the user presents a valid client certificate. See the `Authenticator.cert_auth_sufficient` method.

### unftp-auth-jsonfile v0.2.0

_tag: unftp-auth-jsonfile-0.2.0_

- Added support for per-user IP allow lists
- [#369](https://github.com/bolcom/libunftp/issues/369) Added support for per-user client certificate CN matching
- [#355](https://github.com/bolcom/libunftp/pull/355) Created a new Docker image that generates PBKDF2 keys for the
  authenticator.

### unftp-auth-* v0.2.0

- compiled unftp-auth-pam against libunftp v0.18.0
- compiled unftp-auth-rest against libunftp v0.18.0

### unftp-sbe-* v0.2.0

- compiled unftp-sbe-fs against libunftp v0.18.0
- compiled unftp-sbe-gcs against libunftp v0.18.0

## 2021-05-22 unftp-sbe-gcs v0.1.1

_tag: unftp-sbe-gcs-0.1.1_

- Added an extension trait that adds a `Server::with_gcs` constructor.
- Added support for the `SITE MD5` FTP command. Also
  see [Server::sitemd5](https://docs.rs/libunftp/0.17.4/libunftp/struct.Server.html#method.sitemd5) in libunftp.

## 2021-05-22 libunftp 0.17.4

_tag: libunftp-0.17.4_

- Added a new `SITE MD5` command that allows FTP clients to obtain the MD5 checksum of a remote file. The feature is
  disabled for anonymous users by default.
  See [Server::sitemd5](https://docs.rs/libunftp/0.17.4/libunftp/struct.Server.html#method.sitemd5).

## 2021-05-02 libunftp v0.17.3

_tag: libunftp-0.17.3_

- Added Mutual TLS support.

## 2021-05-01 unftp-auth-jsonfile v0.1.1

_tag: unftp-auth-jsonfile-0.1.1_

- Added support for PBKDF2 encoded passwords

## 2021-04-25 libunftp v0.17.2

_tag: libunftp-0.17.2_

- Fixed output formatting of the FEAT command.
- Fixed the SIZE command that wrongly took the REST restart position into account and also caused number overflows
  because of that.
- Removed panics that could happen when failing to load the TLS certificate or key, these errors are now propagated via
  the `Server::listen` method.
- Implemented TLS session resumption with server side session IDs.
- Implemented TLS session resumption with [tickets](https://tools.ietf.org/html/rfc5077).
- Added the `Server::ftps_tls_flags` method to allow switching TLS features on or off.

## 2021-04-18 libunftp v0.17.1

_tag: libunftp-0.17.1_

Changes in this release:

- [#327](https://github.com/bolcom/libunftp/issues/327) Allow PROT and PBSZ without requiring authentication.
- [#330](https://github.com/bolcom/libunftp/pull/330) Load TLS certificates only once at startup instead of on every
  connect.

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
