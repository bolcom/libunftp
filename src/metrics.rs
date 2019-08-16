use crate::commands::Command;
use crate::reply::{Reply, ReplyCode};
use crate::server::error::FTPErrorKind;
use crate::server::{Event, InternalMsg};
use lazy_static::*;
use prometheus::{
    __register_counter_vec, opts, register_int_counter, register_int_counter_vec, IntCounter,
    IntCounterVec, __register_counter,
};

lazy_static! {
    static ref FTP_AUTH_FAILURES: IntCounter = register_int_counter!(opts!(
        "ftp_auth_failures",
        "Total number of authentication failures."
    ))
    .unwrap();
    static ref FTP_SESSIONS: IntCounter =
        register_int_counter!(opts!("ftp_sessions_total", "Total number of FTP sessions."))
            .unwrap();
    static ref FTP_BACKEND_WRITE_BYTES: IntCounter = register_int_counter!(opts!(
        "ftp_backend_write_bytes",
        "Total number of bytes written to the backend."
    ))
    .unwrap();
    static ref FTP_BACKEND_READ_BYTES: IntCounter = register_int_counter!(opts!(
        "ftp_backend_read_bytes",
        "Total number of bytes retrieved from the backend."
    ))
    .unwrap();
    static ref FTP_BACKEND_WRITE_FILES: IntCounter = register_int_counter!(opts!(
        "ftp_backend_write_files",
        "Total number of files written to the backend."
    ))
    .unwrap();
    static ref FTP_BACKEND_READ_FILES: IntCounter = register_int_counter!(opts!(
        "ftp_backend_read_files",
        "Total number of files retrieved from the backend."
    ))
    .unwrap();
    static ref FTP_COMMAND_TOTAL: IntCounterVec = register_int_counter_vec!(
        "ftp_command_total",
        "Total number of commands received.",
        &["command"]
    )
    .unwrap();
    static ref FTP_REPLY_TOTAL: IntCounterVec = register_int_counter_vec!(
        "ftp_reply_total",
        "Total number of reply codes server sent to clients.",
        &["range"]
    )
    .unwrap();
    static ref FTP_ERROR_TOTAL: IntCounterVec = register_int_counter_vec!(
        "ftp_error_total",
        "Total number of errors encountered.",
        &["type"]
    )
    .unwrap();
}

/// Add a metric for an event.
pub fn add_event_metric(event: &Event) {
    match event {
        Event::Command(cmd) => {
            add_command_metric(&cmd);
        }
        Event::InternalMsg(msg) => match msg {
            InternalMsg::SendData { bytes } => {
                FTP_BACKEND_READ_BYTES.inc_by(*bytes);
                FTP_BACKEND_READ_FILES.inc();
            }
            InternalMsg::WrittenData { bytes } => {
                FTP_BACKEND_WRITE_BYTES.inc_by(*bytes);
                FTP_BACKEND_WRITE_FILES.inc();
            }
            _ => {}
        },
    }
}

/// Add a metric for an FTP server error.
pub fn add_error_metric(error: &FTPErrorKind) {
    match error {
        FTPErrorKind::IOError => {
            FTP_ERROR_TOTAL.with_label_values(&["io"]).inc();
        }
        FTPErrorKind::ParseError => {
            FTP_ERROR_TOTAL.with_label_values(&["parse"]).inc();
        }
        FTPErrorKind::InternalServerError => {
            FTP_ERROR_TOTAL.with_label_values(&["internal"]).inc();
        }
        FTPErrorKind::AuthenticationError => {
            FTP_ERROR_TOTAL.with_label_values(&["authentication"]).inc();
        }
        FTPErrorKind::InternalMsgError => {
            FTP_ERROR_TOTAL.with_label_values(&["internalmsg"]).inc();
        }
        FTPErrorKind::UTF8Error => {
            FTP_ERROR_TOTAL.with_label_values(&["utf8"]).inc();
        }
        FTPErrorKind::UnknownCommand { .. } => {
            FTP_ERROR_TOTAL.with_label_values(&["unknown_cmd"]).inc();
        }
        FTPErrorKind::InvalidCommand => {
            FTP_ERROR_TOTAL.with_label_values(&["invalid_cmd"]).inc();
        }
    }
}

fn add_command_metric(cmd: &Command) {
    match cmd {
        Command::User { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["user"]).inc();
        }
        Command::Pass { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["pass"]).inc();
        }
        Command::Acct { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["acct"]).inc();
        }
        Command::Syst => {
            FTP_COMMAND_TOTAL.with_label_values(&["syst"]).inc();
        }
        Command::Stat { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["stat"]).inc();
        }
        Command::Type => {
            FTP_COMMAND_TOTAL.with_label_values(&["type"]).inc();
        }
        Command::Stru { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["stru"]).inc();
        }
        Command::Mode { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["mode"]).inc();
        }
        Command::Help => {
            FTP_COMMAND_TOTAL.with_label_values(&["help"]).inc();
        }
        Command::Noop => {
            FTP_COMMAND_TOTAL.with_label_values(&["noop"]).inc();
        }
        Command::Pasv => {
            FTP_COMMAND_TOTAL.with_label_values(&["pasv"]).inc();
        }
        Command::Port => {
            FTP_COMMAND_TOTAL.with_label_values(&["port"]).inc();
        }
        Command::Retr { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["retr"]).inc();
        }
        Command::Stor { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["stor"]).inc();
        }
        Command::List { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["list"]).inc();
        }
        Command::Nlst { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["nlst"]).inc();
        }
        Command::Feat => {
            FTP_COMMAND_TOTAL.with_label_values(&["feat"]).inc();
        }
        Command::Pwd => {
            FTP_COMMAND_TOTAL.with_label_values(&["pwd"]).inc();
        }
        Command::Cwd { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["cwd"]).inc();
        }
        Command::Cdup => {
            FTP_COMMAND_TOTAL.with_label_values(&["cdup"]).inc();
        }
        Command::Opts { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["opts"]).inc();
        }
        Command::Dele { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["dele"]).inc();
        }
        Command::Rmd { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["rmd"]).inc();
        }
        Command::Quit => {
            FTP_COMMAND_TOTAL.with_label_values(&["quit"]).inc();
        }
        Command::Mkd { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["mkd"]).inc();
        }
        Command::Allo {} => {
            FTP_COMMAND_TOTAL.with_label_values(&["allo"]).inc();
        }
        Command::Abor => {
            FTP_COMMAND_TOTAL.with_label_values(&["abor"]).inc();
        }
        Command::Stou => {
            FTP_COMMAND_TOTAL.with_label_values(&["stou"]).inc();
        }
        Command::Rnfr { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["rnfr"]).inc();
        }
        Command::Rnto { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["rnto"]).inc();
        }
        Command::Auth { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["auth"]).inc();
        }
        Command::PBSZ { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["pbsz"]).inc();
        }
        Command::CCC { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["ccc"]).inc();
        }
        Command::CDC { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["cdc"]).inc();
        }
        Command::PROT { .. } => {
            FTP_COMMAND_TOTAL.with_label_values(&["prot"]).inc();
        }
    }
}

/// Add a metric for a reply.
pub fn add_reply_metric(reply: &Reply) {
    match reply {
        Reply::None => {}
        Reply::CodeAndMsg { code, msg: _ } => add_replycode_metric(&code),
        Reply::MultiLine { code, lines: _ } => add_replycode_metric(&code),
    }
}

fn add_replycode_metric(code: &ReplyCode) {
    let range = format!("{}xx", *code as u32 / 100 % 10);
    FTP_REPLY_TOTAL.with_label_values(&[&range]).inc();
}
