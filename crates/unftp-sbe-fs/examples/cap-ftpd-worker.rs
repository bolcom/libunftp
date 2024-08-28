//! A libexec helper for cap-std.  It takes an int as $1 which is interpreted as
//! a file descriptor for an already-connected an authenticated control socket.
//! Do not invoke this program directly.  Rather, invoke it by examples/cap-ftpd
#![allow(unsafe_code)]
use cfg_if::cfg_if;
use libunftp::Server;
use std::{
    env,
    os::fd::{FromRawFd, RawFd},
    process::exit,
    sync::{Arc, Mutex},
};
use tokio::net::TcpStream;
use unftp_sbe_fs::Filesystem;

mod auth {
    use std::{
        collections::HashMap,
        fmt, fs,
        io::Read,
        path::{Path, PathBuf},
        time::Duration,
    };

    use async_trait::async_trait;
    use libunftp::auth::{AuthenticationError, Authenticator, DefaultUser, UserDetail};
    use serde::Deserialize;
    use tokio::time::sleep;

    #[derive(Debug)]
    pub struct User {
        username: String,
        home: Option<PathBuf>,
    }

    #[derive(Deserialize, Clone, Debug)]
    #[serde(untagged)]
    enum Credentials {
        Plaintext {
            username: String,
            password: Option<String>,
            home: Option<PathBuf>,
        },
    }

    #[derive(Clone, Debug)]
    struct UserCreds {
        pub password: Option<String>,
        pub home: Option<PathBuf>,
    }

    impl User {
        fn new(username: &str, home: &Option<PathBuf>) -> Self {
            User {
                username: username.to_owned(),
                home: home.clone(),
            }
        }
    }

    impl UserDetail for User {
        fn home(&self) -> Option<&Path> {
            match &self.home {
                None => None,
                Some(p) => Some(p.as_path()),
            }
        }
    }

    impl fmt::Display for User {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.username.as_str())
        }
    }

    /// This structure implements the libunftp `Authenticator` trait
    #[derive(Clone, Debug)]
    pub struct JsonFileAuthenticator {
        credentials_map: HashMap<String, UserCreds>,
    }

    impl JsonFileAuthenticator {
        /// Initialize a new [`JsonFileAuthenticator`] from file.
        pub fn from_file<P: AsRef<Path>>(filename: P) -> Result<Self, Box<dyn std::error::Error>> {
            let mut f = fs::File::open(&filename)?;

            let mut json = String::new();
            f.read_to_string(&mut json)?;

            Self::from_json(json)
        }

        /// Initialize a new [`JsonFileAuthenticator`] from json string.
        pub fn from_json<T: Into<String>>(json: T) -> Result<Self, Box<dyn std::error::Error>> {
            let credentials_list: Vec<Credentials> = serde_json::from_str::<Vec<Credentials>>(&json.into())?;
            let map: Result<HashMap<String, UserCreds>, _> = credentials_list.into_iter().map(Self::list_entry_to_map_entry).collect();
            Ok(JsonFileAuthenticator { credentials_map: map? })
        }

        fn list_entry_to_map_entry(user_info: Credentials) -> Result<(String, UserCreds), Box<dyn std::error::Error>> {
            let map_entry = match user_info {
                Credentials::Plaintext { username, password, home } => (username.clone(), UserCreds { password, home }),
            };
            Ok(map_entry)
        }

        fn check_password(given_password: &str, actual_password: &Option<String>) -> Result<(), ()> {
            if let Some(pwd) = actual_password {
                if pwd == given_password {
                    Ok(())
                } else {
                    Err(())
                }
            } else {
                Err(())
            }
        }
    }

    #[async_trait]
    impl Authenticator<User> for JsonFileAuthenticator {
        #[tracing_attributes::instrument]
        async fn authenticate(&self, username: &str, creds: &libunftp::auth::Credentials) -> Result<User, AuthenticationError> {
            let res = if let Some(actual_creds) = self.credentials_map.get(username) {
                let pass_check_result = match &creds.password {
                    Some(ref given_password) => {
                        if Self::check_password(given_password, &actual_creds.password).is_ok() {
                            Some(Ok(User::new(username, &actual_creds.home)))
                        } else {
                            Some(Err(AuthenticationError::BadPassword))
                        }
                    }
                    None => None,
                };

                match pass_check_result {
                    None => Err(AuthenticationError::BadPassword),
                    Some(pass_res) => {
                        if pass_res.is_ok() {
                            Ok(User::new(username, &actual_creds.home))
                        } else {
                            pass_res
                        }
                    }
                }
            } else {
                Err(AuthenticationError::BadUser)
            };

            if res.is_err() {
                sleep(Duration::from_millis(1500)).await;
            }

            res
        }

        fn name(&self) -> &str {
            std::any::type_name::<Self>()
        }
    }

    #[async_trait]
    impl Authenticator<DefaultUser> for JsonFileAuthenticator {
        #[tracing_attributes::instrument]
        async fn authenticate(&self, username: &str, creds: &libunftp::auth::Credentials) -> Result<DefaultUser, AuthenticationError> {
            let _: User = self.authenticate(username, creds).await?;
            Ok(DefaultUser {})
        }
    }
}

use auth::{JsonFileAuthenticator, User};

cfg_if! {
    if #[cfg(target_os = "freebsd")] {
        use std::{
            io,
            net::IpAddr,
            ops::Range
        };
        use async_trait::async_trait;
        use capsicum::casper::Casper;
        use capsicum_net::{CapNetAgent, CasperExt, tokio::TcpSocketExt};
        use tokio::net::TcpSocket;

        #[derive(Debug)]
        struct CapBinder {
            agent: CapNetAgent
        }

        impl CapBinder {
            fn new(agent: CapNetAgent) -> Self {
                Self{agent}
            }
        }

        #[async_trait]
        impl libunftp::options::Binder for CapBinder {
            async fn bind(&mut self, local_addr: IpAddr, passive_ports: Range<u16>) -> io::Result<TcpSocket> {
                const BIND_RETRIES: u8 = 10;

                for _ in 1..BIND_RETRIES {
                    let mut data = [0u8; 2];
                    getrandom::getrandom(&mut data).expect("Error generating random port");
                    let r16 = u16::from_ne_bytes(data);
                    let p = passive_ports.start + r16 % (passive_ports.end - passive_ports.start);
                    let socket = TcpSocket::new_v4()?;
                    let addr = std::net::SocketAddr::new(local_addr, p);
                    match socket.cap_bind(&mut self.agent, addr) {
                        Ok(()) => return Ok(socket),
                        Err(_) => todo!()
                    }
                }
                panic!()
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
#[allow(unused_mut)] // Not unused on all OSes.
async fn main() {
    println!("Starting helper");
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: {} <AUTH_FILE> <FD>", args[0]);
        exit(2);
    }
    let fd: RawFd = if let Ok(fd) = args[2].parse() {
        fd
    } else {
        eprintln!("Usage: {} <FD>\nFD must be numeric", args[0]);
        exit(2)
    };

    let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };

    let control_sock = TcpStream::from_std(std_stream).unwrap();

    let auth = Arc::new(JsonFileAuthenticator::from_file(args[1].clone()).unwrap());
    // XXX This would be a lot easier if the libunftp API allowed creating the
    // storage just before calling service.
    let storage = Mutex::new(Some(Filesystem::new(std::env::temp_dir()).unwrap()));
    let sgen = Box::new(move || storage.lock().unwrap().take().unwrap());

    let mut sb = libunftp::ServerBuilder::with_authenticator(sgen, auth);
    cfg_if! {
        if #[cfg(target_os = "freebsd")] {
            // Safe because we're single-threaded
            let mut casper = unsafe { Casper::new().unwrap() };

            let cap_net = casper.net().unwrap();
            let binder = CapBinder::new(cap_net);
            sb = sb.binder(binder);
        }
    }
    let server: Server<Filesystem, User> = sb.build().unwrap();
    cfg_if! {
        if #[cfg(target_os = "freebsd")] {
            capsicum::enter().unwrap();
        }
    }
    server.service(control_sock).await.unwrap()
}
