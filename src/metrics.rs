//! Contains the `add...metric` functions that are used for gathering metrics.

use crate::server::{
    commands::Command,
    reply::{Reply, ReplyCode},
    Event, FTPErrorKind, InternalMsg,
};

use lazy_static::*;
use prometheus::{
    __register_counter_vec, __register_gauge, opts, register_counter, register_int_counter, register_int_counter_vec, register_int_gauge, IntCounter,
    IntCounterVec, IntGauge,
};

lazy_static! {
    static ref FTP_AUTH_FAILURES: IntCounter = register_int_counter!(opts!("ftp_auth_failures", "Total number of authentication failures.")).unwrap();
    static ref FTP_SESSIONS: IntGauge = register_int_gauge!(opts!("ftp_sessions_total", "Total number of FTP sessions.")).unwrap();
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

/// Increase the metrics gauge for client sessions
pub fn inc_session() {
    FTP_SESSIONS.inc();
}

/// Decrease the metrics gauge for client sessions
pub fn dec_session() {
    FTP_SESSIONS.dec();
}

/// Add a metric for an FTP server error.
pub fn add_error_metric(error: &FTPErrorKind) {
    let error_str = error.to_string();
    let label = error_str.split_whitespace().next().unwrap_or("unknown").to_lowercase();
    FTP_ERROR_TOTAL.with_label_values(&[&label]).inc();
}

fn add_command_metric(cmd: &Command) {
    let cmd_str = cmd.to_string();
    let label = cmd_str.split_whitespace().next().unwrap_or("unknown").to_lowercase();
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
