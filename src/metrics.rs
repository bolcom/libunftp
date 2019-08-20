use crate::server::{
    commands::Command,
    reply::{Reply, ReplyCode},
    Event, FTPErrorKind,
};

use lazy_static::*;
use prometheus::{__register_counter_vec, opts, register_int_counter, register_int_counter_vec, IntCounter, IntCounterVec, __register_counter};

lazy_static! {
    static ref FTP_AUTH_FAILURES: IntCounter = register_int_counter!(opts!("ftp_auth_failures", "Total number of authentication failures.")).unwrap();
    static ref FTP_SESSIONS: IntCounter = register_int_counter!(opts!("ftp_sessions_total", "Total number of FTP sessions.")).unwrap();
    static ref FTP_BACKEND_WRITE_BYTES: IntCounter =
        register_int_counter!(opts!("ftp_backend_write_bytes", "Total number of bytes written to the backend.")).unwrap();
    static ref FTP_BACKEND_READ_BYTES: IntCounter =
        register_int_counter!(opts!("ftp_backend_read_bytes", "Total number of bytes retrieved from the backend.")).unwrap();
    static ref FTP_BACKEND_WRITE_FILES: IntCounter =
        register_int_counter!(opts!("ftp_backend_write_files", "Total number of files written to the backend.")).unwrap();
    static ref FTP_BACKEND_READ_FILES: IntCounter =
        register_int_counter!(opts!("ftp_backend_read_files", "Total number of files retrieved from the backend.")).unwrap();
    static ref FTP_COMMAND_TOTAL: IntCounterVec = register_int_counter_vec!("ftp_command_total", "Total number of commands received.", &["command"]).unwrap();
    static ref FTP_REPLY_TOTAL: IntCounterVec =
        register_int_counter_vec!("ftp_reply_total", "Total number of reply codes server sent to clients.", &["range"]).unwrap();
    static ref FTP_ERROR_TOTAL: IntCounterVec = register_int_counter_vec!("ftp_error_total", "Total number of errors encountered.", &["type"]).unwrap();
}

/// Add a metric for an event.
pub fn add_event_metric(event: &Event) {
    match event {
        Event::Command(cmd) => {
            add_command_metric(&cmd);
        }
    }
}

/// Add a metric for an FTP server error.
pub fn add_error_metric(error: &FTPErrorKind) {
    let error_str = error.to_string();
    let label = error_str.split_whitespace().nth(0).unwrap_or("unknown").to_lowercase();
    FTP_ERROR_TOTAL.with_label_values(&[&label]).inc();
}

fn add_command_metric(cmd: &Command) {
    let cmd_str = cmd.to_string();
    let label = cmd_str.split_whitespace().nth(0).unwrap_or("unknown").to_lowercase();
    FTP_COMMAND_TOTAL.with_label_values(&[&label]).inc();
}

/// Add a metric for a reply.
pub fn add_reply_metric(reply: &Reply) {
    match *reply {
        Reply::None => {}
        Reply::CodeAndMsg { code, .. } => add_replycode_metric(code),
        Reply::MultiLine { code, .. } => add_replycode_metric(code),
    }
}

fn add_replycode_metric(code: ReplyCode) {
    let range = format!("{}xx", code as u32 / 100 % 10);
    FTP_REPLY_TOTAL.with_label_values(&[&range]).inc();
}
