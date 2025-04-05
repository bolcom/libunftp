use crate::server::{
    controlchan::commands::{AuthParam, ModeParam, Opt, ProtParam, StruParam},
    password::Password,
};

use bytes::Bytes;
use std::{fmt, path::PathBuf};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Command {
    User {
        /// The bytes making up the actual username.
        username: Bytes,
    },
    Pass {
        /// The bytes making up the actual password.
        password: Password,
    },
    Acct {
        /// The bytes making up the account about which information is requested.
        account: Bytes,
    },
    Syst,
    Stat {
        /// The bytes making up the path about which information is requested, if given.
        path: Option<Bytes>,
    },
    Type,
    Stru {
        /// The structure to which the client would like to switch. Only the `File` structure is
        /// supported by us.
        structure: StruParam,
    },
    Mode {
        /// The transfer mode to which the client would like to switch. Only the `Stream` mode is
        /// supported by us.
        mode: ModeParam,
    },
    Help,
    Noop,
    Pasv,
    Epsv,
    Port {
        /// The address to use to make an active connection to the client
        addr: String,
    },
    Retr {
        /// The path to the file the client would like to retrieve.
        path: String,
    },
    Stor {
        /// The path to the file the client would like to store.
        path: String,
    },
    List {
        /// Arguments passed along with the list command.
        options: Option<String>,
        /// The path of the file/directory the clients wants to list
        path: Option<String>,
    },
    Nlst {
        /// The path of the file/directory the clients wants to list.
        path: Option<String>,
    },
    /// Machine List Single (MLST) command for getting machine-readable information about a single file/directory
    Mlst {
        /// The path of the file/directory to get information about
        path: Option<String>,
    },
    Feat,
    Pwd,
    Cwd {
        /// The path the client would like to change directory to.
        path: PathBuf,
    },
    Cdup,
    Opts {
        /// The option the client wants to set
        option: Opt,
    },
    Dele {
        /// The (regular) file to delete.
        path: String,
    },
    Rmd {
        /// The (regular) directory to delete.
        path: String,
    },
    Quit,
    Mkd {
        /// The path to the directory the client wants to create.
        path: PathBuf,
    },
    Allo {
        // The `ALLO` command can actually have an optional argument, but since we regard `ALLO`
        // as noop, we won't even parse it.
    },
    Abor,
    Stou,
    Rnfr {
        /// The file to be renamed
        file: PathBuf,
    },
    Rnto {
        /// The filename to rename to
        file: PathBuf,
    },
    Auth {
        protocol: AuthParam,
    },
    Ccc,
    Pbsz {},
    Prot {
        param: ProtParam,
    },
    Size {
        file: PathBuf,
    },
    Rest {
        offset: u64,
    },
    /// Modification Time (MDTM) as specified in RFC 3659.
    /// This command can be used to determine when a file in the server NVFS was last modified.
    Mdtm {
        file: PathBuf,
    },
    Md5 {
        file: PathBuf,
    },
    Other {
        command_name: String,
        arguments: String,
    },
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
