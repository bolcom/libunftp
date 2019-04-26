use crate::commands::Command;
use crate::prometheus::IntCounter;
use crate::reply::{Reply, ReplyCode};
use crate::server::{Event, FTPErrorKind, InternalMsg};

// We have to break up the creation of metrics with lazy_static! or
// else we get an error during compilation that the recursive limit
// has been reached.
//
// Create basic metrics.
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
}

// Create metrics for the different FTP commands.
lazy_static! {
    static ref FTP_COMMAND_USER_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_user_total",
        "Total number of 'USER' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_PASS_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_pass_total",
        "Total number of 'PASS' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_ACCT_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_acct_total",
        "Total number of 'ACCT' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_SYST_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_syst_total",
        "Total number of 'SYST' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_STAT_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_stat_total",
        "Total number of 'STAT' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_TYPE_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_type_total",
        "Total number of 'TYPE' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_STRU_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_stru_total",
        "Total number of 'STRU' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_MODE_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_mode_total",
        "Total number of 'MODE' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_HELP_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_help_total",
        "Total number of 'HELP' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_NOOP_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_noop_total",
        "Total number of 'NOOP' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_PASV_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_pasv_total",
        "Total number of 'PASV' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_PORT_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_port_total",
        "Total number of 'PORT' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_RETR_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_retr_total",
        "Total number of 'RETR' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_STOR_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_stor_total",
        "Total number of 'STOR' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_LIST_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_list_total",
        "Total number of 'LIST' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_NLST_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_nlst_total",
        "Total number of 'NLST' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_FEAT_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_feat_total",
        "Total number of 'FEAT' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_PWD_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_pwd_total",
        "Total number of 'PWD' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_CWD_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_cwd_total",
        "Total number of 'CWD' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_CDUP_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_cdup_total",
        "Total number of 'CDUP' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_OPTS_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_opts_total",
        "Total number of 'OPTS' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_DELE_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_dele_total",
        "Total number of 'DELE' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_QUIT_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_quit_total",
        "Total number of 'QUIT' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_MKD_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_mkd_total",
        "Total number of 'MKD' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_ALLO_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_allo_total",
        "Total number of 'ALLO' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_ABOR_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_abor_total",
        "Total number of 'ABOR' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_STOU_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_stou_total",
        "Total number of 'STOU' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_RNFR_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_rnfr_total",
        "Total number of 'RNFR' commands received."
    ))
    .unwrap();
    static ref FTP_COMMAND_RNTO_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_command_rnto_total",
        "Total number of 'RNTO' commands received."
    ))
    .unwrap();
}

// Create metrics for aggregating the reply codes sent by the server.
lazy_static! {
    static ref FTP_REPLY_1XX_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_reply_1xx_total",
        "Total number of 1xx reply codes server sent to clients."
    ))
    .unwrap();
    static ref FTP_REPLY_2XX_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_reply_2xx_total",
        "Total number of 2xx reply codes server sent to clients."
    ))
    .unwrap();
    static ref FTP_REPLY_3XX_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_reply_3xx_total",
        "Total number of 3xx reply codes server sent to clients."
    ))
    .unwrap();
    static ref FTP_REPLY_4XX_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_reply_4xx_total",
        "Total number of 4xx reply codes server sent to clients."
    ))
    .unwrap();
    static ref FTP_REPLY_5XX_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_reply_5xx_total",
        "Total number of 5xx reply codes server sent to clients."
    ))
    .unwrap();
    static ref FTP_REPLY_6XX_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_reply_6xx_total",
        "Total number of 6xx reply codes server sent to clients."
    ))
    .unwrap();
}

// Create metrics for server errors.
lazy_static! {
    static ref FTP_ERROR_IO_TOTAL: IntCounter =
        register_int_counter!(opts!("ftp_error_io_total", "Total number of IO errors.")).unwrap();
    static ref FTP_ERROR_PARSE_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_parse_total",
        "Total number of command parse errors."
    ))
    .unwrap();
    static ref FTP_ERROR_INTERNAL_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_internal_total",
        "Total number of internal server errors."
    ))
    .unwrap();
    static ref FTP_ERROR_AUTHENTICATION_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_authentication_total",
        "Total number of authentication backend errors."
    ))
    .unwrap();
    static ref FTP_ERROR_INTERNALMSG_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_internalmsg_total",
        "Total number of data channel mapping errors."
    ))
    .unwrap();
    static ref FTP_ERROR_UTF8_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_utf8_total",
        "Total number of commands with non-UTF8 characters."
    ))
    .unwrap();
    static ref FTP_ERROR_UNKNOWN_CMD_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_unknown_cmd_total",
        "Total number of unknown commands received."
    ))
    .unwrap();
    static ref FTP_ERROR_INVALID_CMD_TOTAL: IntCounter = register_int_counter!(opts!(
        "ftp_error_invalid_cmd_total",
        "Total number of invalid commands received."
    ))
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
            FTP_ERROR_IO_TOTAL.inc();
        }
        FTPErrorKind::ParseError => {
            FTP_ERROR_PARSE_TOTAL.inc();
        }
        FTPErrorKind::InternalServerError => {
            FTP_ERROR_INTERNAL_TOTAL.inc();
        }
        FTPErrorKind::AuthenticationError => {
            FTP_ERROR_AUTHENTICATION_TOTAL.inc();
        }
        FTPErrorKind::InternalMsgError => {
            FTP_ERROR_INTERNALMSG_TOTAL.inc();
        }
        FTPErrorKind::UTF8Error => {
            FTP_ERROR_UTF8_TOTAL.inc();
        }
        FTPErrorKind::UnknownCommand { .. } => {
            FTP_ERROR_UNKNOWN_CMD_TOTAL.inc();
        }
        FTPErrorKind::InvalidCommand => {
            FTP_ERROR_INVALID_CMD_TOTAL.inc();
        }
    }
}

fn add_command_metric(cmd: &Command) {
    match cmd {
        Command::User { .. } => {
            FTP_COMMAND_USER_TOTAL.inc();
        }
        Command::Pass { .. } => {
            FTP_COMMAND_PASS_TOTAL.inc();
        }
        Command::Acct { .. } => {
            FTP_COMMAND_ACCT_TOTAL.inc();
        }
        Command::Syst => {
            FTP_COMMAND_SYST_TOTAL.inc();
        }
        Command::Stat { .. } => {
            FTP_COMMAND_STAT_TOTAL.inc();
        }
        Command::Type => {
            FTP_COMMAND_TYPE_TOTAL.inc();
        }
        Command::Stru { .. } => {
            FTP_COMMAND_STRU_TOTAL.inc();
        }
        Command::Mode { .. } => {
            FTP_COMMAND_MODE_TOTAL.inc();
        }
        Command::Help => {
            FTP_COMMAND_HELP_TOTAL.inc();
        }
        Command::Noop => {
            FTP_COMMAND_NOOP_TOTAL.inc();
        }
        Command::Pasv => {
            FTP_COMMAND_PASV_TOTAL.inc();
        }
        Command::Port => {
            FTP_COMMAND_PORT_TOTAL.inc();
        }
        Command::Retr { .. } => {
            FTP_COMMAND_RETR_TOTAL.inc();
        }
        Command::Stor { .. } => {
            FTP_COMMAND_STOR_TOTAL.inc();
        }
        Command::List { .. } => {
            FTP_COMMAND_LIST_TOTAL.inc();
        }
        Command::Nlst { .. } => {
            FTP_COMMAND_NLST_TOTAL.inc();
        }
        Command::Feat => {
            FTP_COMMAND_FEAT_TOTAL.inc();
        }
        Command::Pwd => {
            FTP_COMMAND_PWD_TOTAL.inc();
        }
        Command::Cwd { .. } => {
            FTP_COMMAND_CWD_TOTAL.inc();
        }
        Command::Cdup => {
            FTP_COMMAND_CDUP_TOTAL.inc();
        }
        Command::Opts { .. } => {
            FTP_COMMAND_OPTS_TOTAL.inc();
        }
        Command::Dele { .. } => {
            FTP_COMMAND_DELE_TOTAL.inc();
        }
        Command::Quit => {
            FTP_COMMAND_QUIT_TOTAL.inc();
        }
        Command::Mkd { .. } => {
            FTP_COMMAND_MKD_TOTAL.inc();
        }
        Command::Allo {} => {
            FTP_COMMAND_ALLO_TOTAL.inc();
        }
        Command::Abor => {
            FTP_COMMAND_ABOR_TOTAL.inc();
        }
        Command::Stou => {
            FTP_COMMAND_STOU_TOTAL.inc();
        }
        Command::Rnfr { .. } => {
            FTP_COMMAND_RNFR_TOTAL.inc();
        }
        Command::Rnto { .. } => {
            FTP_COMMAND_RNTO_TOTAL.inc();
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
    match *code as u32 / 100 % 10 {
        1 => {
            FTP_REPLY_1XX_TOTAL.inc();
        }
        2 => {
            FTP_REPLY_2XX_TOTAL.inc();
        }
        3 => {
            FTP_REPLY_3XX_TOTAL.inc();
        }
        4 => {
            FTP_REPLY_4XX_TOTAL.inc();
        }
        5 => {
            FTP_REPLY_5XX_TOTAL.inc();
        }
        6 => {
            FTP_REPLY_6XX_TOTAL.inc();
        }
        _ => {}
    }
}
