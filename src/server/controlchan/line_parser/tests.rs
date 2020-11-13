use super::error::{ParseError, ParseErrorKind, Result};
use crate::server::controlchan::{
    command::Command,
    commands::{AuthParam, ModeParam, Opt, StruParam},
    line_parser::parser::parse,
};

use pretty_assertions::assert_eq;

#[test]
fn parse_user_cmd_crnl() {
    let input = "USER Dolores\r\n";
    assert_eq!(parse(input).unwrap(), Command::User { username: "Dolores".into() });
}

#[test]
// TODO: According to RFC 959, verbs should be interpreted without regards to case
fn parse_user_cmd_mixed_case() {
    let input = "uSeR Dolores\r\n";
    assert_eq!(parse(input).unwrap(), Command::User { username: "Dolores".into() });
}

#[test]
fn parse_user_lowercase() {
    let input = "user Dolores\r\n";
    assert_eq!(parse(input).unwrap(), Command::User { username: "Dolores".into() });
}

#[test]
// Not all clients include the (actually mandatory) '\r'
fn parse_user_cmd_nl() {
    let input = "USER Dolores\n";
    assert_eq!(parse(input).unwrap(), Command::User { username: "Dolores".into() });
}

#[test]
// Although we accept requests ending in only '\n', we won't accept requests ending only in '\r'
fn parse_user_cmd_cr() {
    let input = "USER Dolores\r";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidEOL)));
}

#[test]
// We should fail if the request does not end in '\n' or '\r'
fn parse_user_cmd_no_eol() {
    let input = "USER Dolores";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidEOL)));
}

#[test]
// We should skip only one space after a token, to allow for tokens starting with a space.
fn parse_user_cmd_double_space() {
    let input = "USER  Dolores\r\n";
    assert_eq!(parse(input).unwrap(), Command::User { username: " Dolores".into() });
}

#[test]
fn parse_user_cmd_whitespace() {
    let input = "USER Dolores Abernathy\r\n";
    assert_eq!(
        parse(input).unwrap(),
        Command::User {
            username: "Dolores Abernathy".into()
        }
    );
}

#[test]
fn parse_pass_cmd_crnl() {
    let input = "PASS s3cr3t\r\n";
    assert_eq!(parse(input).unwrap(), Command::Pass { password: "s3cr3t".into() });
}

#[test]
fn parse_pass_cmd_whitespace() {
    let input = "PASS s3cr#t p@S$w0rd\r\n";
    assert_eq!(
        parse(input).unwrap(),
        Command::Pass {
            password: "s3cr#t p@S$w0rd".into()
        }
    );
}

#[test]
fn parse_acct() {
    let input = "ACCT Teddy\r\n";
    assert_eq!(parse(input).unwrap(), Command::Acct { account: "Teddy".into() });
}

#[test]
fn parse_stru_no_params() {
    let input = "STRU\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_stru_f() {
    let input = "STRU F\r\n";
    assert_eq!(parse(input).unwrap(), Command::Stru { structure: StruParam::File });
}

#[test]
fn parse_stru_r() {
    let input = "STRU R\r\n";
    assert_eq!(parse(input).unwrap(), Command::Stru { structure: StruParam::Record });
}

#[test]
fn parse_stru_p() {
    let input = "STRU P\r\n";
    assert_eq!(parse(input).unwrap(), Command::Stru { structure: StruParam::Page });
}

#[test]
fn parse_stru_garbage() {
    let input = "STRU FSK\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "STRU F lskdjf\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "STRU\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_mode_s() {
    let input = "MODE S\r\n";
    assert_eq!(parse(input).unwrap(), Command::Mode { mode: ModeParam::Stream });
}

#[test]
fn parse_mode_b() {
    let input = "MODE B\r\n";
    assert_eq!(parse(input).unwrap(), Command::Mode { mode: ModeParam::Block });
}

#[test]
fn parse_mode_c() {
    let input = "MODE C\r\n";
    assert_eq!(parse(input).unwrap(), Command::Mode { mode: ModeParam::Compressed });
}

#[test]
fn parse_mode_garbage() {
    let input = "MODE SKDJF\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "MODE\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "MODE S D\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_help() {
    let input = "HELP\r\n";
    assert_eq!(parse(input).unwrap(), Command::Help);

    let input = "HELP bla\r\n";
    assert_eq!(parse(input).unwrap(), Command::Help);
}

#[test]
fn parse_noop() {
    let input = "NOOP\r\n";
    assert_eq!(parse(input).unwrap(), Command::Noop);

    let input = "NOOP bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_pasv() {
    let input = "PASV\r\n";
    assert_eq!(parse(input).unwrap(), Command::Pasv);

    let input = "PASV bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_port() {
    let input = "PORT\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "PORT a1,a2,a3,a4,p1,p2\r\n";
    assert_eq!(parse(input).unwrap(), Command::Port);
}

#[test]
fn parse_list() {
    struct Test {
        input: &'static str,
        expected_path: Option<&'static str>,
    }

    let tests = [
        Test {
            input: "LIST\r\n",
            expected_path: None,
        },
        Test {
            input: "LIST tmp\r\n",
            expected_path: Some("tmp"),
        },
        Test {
            input: "LIST -la\r\n",
            expected_path: None,
        },
        Test {
            input: "LIST -la tmp\r\n",
            expected_path: Some("tmp"),
        },
        Test {
            input: "LIST -la -x tmp\r\n",
            expected_path: Some("tmp"),
        },
        Test {
            input: "LIST -la -x tmp*\r\n",
            expected_path: Some("tmp*"),
        },
    ];

    for test in tests.iter() {
        assert_eq!(
            parse(test.input),
            Ok(Command::List {
                options: None,
                path: test.expected_path.map(|s| s.to_string()),
            })
        );
    }
}

#[test]
fn parse_feat() {
    let input = "FEAT\r\n";
    assert_eq!(parse(input), Ok(Command::Feat));

    let input = "FEAT bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_pwd() {
    let input = "PWD\r\n";
    assert_eq!(parse(input), Ok(Command::Pwd));

    let input = "PWD bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_cwd() {
    let input = "CWD\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "CWD /tmp\r\n";
    assert_eq!(parse(input), Ok(Command::Cwd { path: "/tmp".into() }));

    let input = "CWD public\r\n";
    assert_eq!(parse(input), Ok(Command::Cwd { path: "public".into() }));
}

#[test]
fn parse_cdup() {
    let input = "CDUP\r\n";
    assert_eq!(parse(input), Ok(Command::Cdup));

    let input = "CDUP bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_opts() {
    let input = "OPTS\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "OPTS bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "OPTS UTF8\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "OPTS UTF8 ON\r\n";
    assert_eq!(
        parse(input),
        Ok(Command::Opts {
            option: Opt::UTF8 { on: true }
        })
    );

    let input = "OPTS UTF8 OFF\r\n";
    assert_eq!(
        parse(input),
        Ok(Command::Opts {
            option: Opt::UTF8 { on: false }
        })
    );
}

#[test]
fn parse_dele() {
    let input = "DELE\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "DELE some_file\r\n";
    assert_eq!(parse(input), Ok(Command::Dele { path: "some_file".into() }));
}

#[test]
fn parse_rmd() {
    let input = "RMD\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "RMD some_directory\r\n";
    assert_eq!(parse(input), Ok(Command::Rmd { path: "some_directory".into() }));
}

#[test]
fn parse_quit() {
    let input = "QUIT\r\n";
    assert_eq!(parse(input), Ok(Command::Quit));

    let input = "QUIT NOW\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_mkd() {
    let input = "MKD\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "MKD bla\r\n";
    assert_eq!(parse(input), Ok(Command::Mkd { path: "bla".into() }));
}

#[test]
fn parse_allo() {
    let input = "ALLO\r\n";
    assert_eq!(parse(input), Ok(Command::Allo {}));

    // This is actually not a valid `ALLO` command, but since we ignore it anyway there is no
    // need to add complexity by actually parsing it.
    let input = "ALLO 5\r\n";
    assert_eq!(parse(input), Ok(Command::Allo {}));

    let input = "ALLO R 5\r\n";
    assert_eq!(parse(input), Ok(Command::Allo {}));
}

#[test]
fn parse_abor() {
    let input = "ABOR\r\n";
    assert_eq!(parse(input), Ok(Command::Abor));

    let input = "ABOR bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_stou() {
    let input = "STOU\r\n";
    assert_eq!(parse(input), Ok(Command::Stou));

    let input = "STOU bla\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
}

#[test]
fn parse_rnfr() {
    let input = "RNFR\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "RNFR dir/file\r\n";
    assert_eq!(parse(input), Ok(Command::Rnfr { file: "dir/file".into() }));

    let input = "RNFR myfile\r\n";
    assert_eq!(parse(input), Ok(Command::Rnfr { file: "myfile".into() }));

    let input = "RNFR this file\r\n";
    assert_eq!(parse(input), Ok(Command::Rnfr { file: "this file".into() }));
}

#[test]
fn parse_rnto() {
    let input = "RNTO\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "RNTO dir/file\r\n";
    assert_eq!(parse(input), Ok(Command::Rnto { file: "dir/file".into() }));

    let input = "RNTO name with spaces\r\n";
    assert_eq!(
        parse(input),
        Ok(Command::Rnto {
            file: "name with spaces".into()
        })
    );

    let input = "RNTO new_name\r\n";
    assert_eq!(parse(input), Ok(Command::Rnto { file: "new_name".into() }));
}

#[test]
fn parse_auth() {
    let input = "AUTH xx\r\n";
    assert_eq!(parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

    let input = "AUTH tls\r\n";
    assert_eq!(parse(input), Ok(Command::Auth { protocol: AuthParam::Tls }));
}

#[test]
fn parse_rest() {
    struct Test {
        input: &'static str,
        expected: Result<Command>,
    }

    let tests = [
        Test {
            input: "REST\r\n",
            expected: Err(ParseErrorKind::InvalidCommand.into()),
        },
        Test {
            input: "REST xxx\r\n",
            expected: Err(ParseErrorKind::InvalidCommand.into()),
        },
        Test {
            input: "REST 1303\r\n",
            expected: Ok(Command::Rest { offset: 1303 }),
        },
        Test {
            input: "REST 1303 343\r\n",
            expected: Err(ParseErrorKind::InvalidCommand.into()),
        },
    ];

    for test in tests.iter() {
        assert_eq!(parse(test.input), test.expected);
    }
}

#[test]
fn parse_mdtm() {
    struct Test {
        input: &'static str,
        expected: Result<Command>,
    }
    let tests = [
        Test {
            input: "MDTM\r\n",
            expected: Err(ParseErrorKind::InvalidCommand.into()),
        },
        Test {
            input: "MDTM file.txt\r\n",
            expected: Ok(Command::Mdtm { file: "file.txt".into() }),
        },
    ];
    for test in tests.iter() {
        assert_eq!(parse(test.input), test.expected);
    }
}
