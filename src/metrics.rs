//! Contains the `add...metric` functions that are used for gathering metrics.

use crate::server::{Command, ControlChanError, ControlChanErrorKind, ControlChanMiddleware, ControlChanMsg, Event, Reply, ReplyCode};

use async_trait::async_trait;
use lazy_static::*;
use prometheus::{opts, register_int_counter, register_int_counter_vec, register_int_gauge, IntCounter, IntCounterVec, IntGauge};

// Control channel middleware that adds metrics
pub struct MetricsMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    pub collect_metrics: bool,
    pub next: Next,
}

#[async_trait]
impl<Next> ControlChanMiddleware for MetricsMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        if self.collect_metrics {
            add_event_metric(&event);
        }
        let (evt_type_label, evt_label) = event_to_labels(&event);
        let result: Result<Reply, ControlChanError> = self.next.handle(event).await;
        if self.collect_metrics {
            match &result {
                Ok(reply) => add_reply_metric(reply, evt_type_label, evt_label),
                Err(e) => add_error_metric(e.kind(), evt_type_label, evt_label),
            }
        }
        result
    }
}

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
    static ref FTP_REPLY_TOTAL: IntCounterVec = register_int_counter_vec!(
        "ftp_reply_total",
        "Total number of reply codes server sent to clients.",
        &["range", "event_type", "event"],
    )
    .unwrap();
    static ref FTP_ERROR_TOTAL: IntCounterVec =
        register_int_counter_vec!("ftp_error_total", "Total number of errors encountered.", &["type", "event_type", "event"]).unwrap();
}

/// Add a metric for an event.
fn add_event_metric(event: &Event) {
    match event {
        Event::Command(cmd) => {
            add_command_metric(cmd);
        }
        Event::InternalMsg(msg) => match msg {
            ControlChanMsg::SentData { bytes, .. } => {
                FTP_BACKEND_READ_BYTES.inc_by(*bytes);
                FTP_BACKEND_READ_FILES.inc();
            }
            ControlChanMsg::WrittenData { bytes, .. } => {
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

fn add_command_metric(cmd: &Command) {
    let label = command_to_label(cmd);
    FTP_COMMAND_TOTAL.with_label_values(&[&label]).inc();
}

/// Error during command processing
fn add_error_metric(error: &ControlChanErrorKind, evt_type_label: String, evt_label: String) {
    let error_str = error.to_string();
    let label = error_str.split_whitespace().next().unwrap_or("unknown").to_lowercase();
    FTP_ERROR_TOTAL.with_label_values(&[&label, &evt_type_label, &evt_label]).inc();
}

/// Add a metric for an FTP reply.
fn add_reply_metric(reply: &Reply, evt_type_label: String, evt_label: String) {
    match *reply {
        Reply::None => {}
        Reply::CodeAndMsg { code, .. } => add_replycode_metric(code, evt_type_label, evt_label),
        Reply::MultiLine { code, .. } => add_replycode_metric(code, evt_type_label, evt_label),
    }
}

fn add_replycode_metric(code: ReplyCode, evt_type_label: String, evt_label: String) {
    let range = format!("{}xx", code as u32 / 100 % 10);
    FTP_REPLY_TOTAL.with_label_values(&[&range, &evt_type_label, &evt_label]).inc();
}

fn event_to_labels(evt: &Event) -> (String, String) {
    let (evt_type_str, evt_str) = match evt {
        Event::Command(cmd) => ("command".into(), cmd.to_string()),
        Event::InternalMsg(msg) => ("ctrl-chan-msg".into(), msg.to_string()),
    };
    let evt_name_str = evt_str.split_whitespace().next().unwrap_or("unknown").to_lowercase();
    (evt_type_str, evt_name_str)
}

fn command_to_label(cmd: &Command) -> String {
    let cmd_str = cmd.to_string();
    cmd_str.split_whitespace().next().unwrap_or("unknown").to_lowercase()
}
