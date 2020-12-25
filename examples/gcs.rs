use clap::{App, Arg};
use std::{error::Error, result::Result};
use tracing::Level;

const BUCKET_NAME: &str = "bucket-name";
const SERVICE_ACCOUNT_KEY: &str = "service-account-key";
const FTPS_CERTS_FILE: &str = "ftps-certs-file";
const FTPS_KEY_FILE: &str = "ftps-key-file";
const BIND_ADDRESS: &str = "127.0.0.1:2121";

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().with_max_level(Level::TRACE).init();

    let matches = App::new("Example for using libunftp with Google Cloud Storage backend with optionally enabling TLS")
        .about("An FTP server that uses Google Cloud Storage as a backend")
        .author("The bol.com unFTP team")
        .arg(
            Arg::with_name(BUCKET_NAME)
                .short("b")
                .long(BUCKET_NAME)
                .value_name("BUCKET_NAME")
                .env("LIBUNFTP_BUCKET_NAME")
                .help("The name of the Google Cloud Storage bucket to be used")
                .required(true),
        )
        .arg(
            Arg::with_name(SERVICE_ACCOUNT_KEY)
                .short("s")
                .long(SERVICE_ACCOUNT_KEY)
                .value_name("SERVICE_ACCOUNT_KEY")
                .env("LIBUNFTP_SERVICE_ACCOUNT_KEY")
                .help("The service account key JSON file of the Google Cloud Storage bucket to be used")
                .required(true),
        )
        .arg(
            Arg::with_name(FTPS_CERTS_FILE)
                .short("c")
                .long(FTPS_CERTS_FILE)
                .value_name("FTPS_CERTS_FILE")
                .env("LIBUNFTP_FTPS_CERTS_FILE")
                .help("The ftps certs file")
                .requires(FTPS_KEY_FILE),
        )
        .arg(
            Arg::with_name(FTPS_KEY_FILE)
                .short("p")
                .long(FTPS_KEY_FILE)
                .value_name("FTPS_KEY_FILE")
                .env("LIBUNFTP_FTPS_KEY_FILE")
                .help("The ftps certs key file")
                .requires(FTPS_CERTS_FILE),
        )
        .get_matches();

    let service_account_key = matches
        .value_of(SERVICE_ACCOUNT_KEY)
        .ok_or("Internal error: use of an undefined command line parameter")?;
    let bucket_name = matches
        .value_of(BUCKET_NAME)
        .ok_or("Internal error: use of an undefined command line parameter")?
        .to_owned();

    let service_account_key = yup_oauth2::read_service_account_key(service_account_key).await?;
    if let Some(ftps_certs_file) = matches.value_of(FTPS_CERTS_FILE) {
        let ftps_key_file = matches
            .value_of(FTPS_KEY_FILE)
            .ok_or("Internal error: use of an undefined command line parameter")?;
        libunftp::Server::new(Box::new(move || {
            libunftp::storage::cloud_storage::CloudStorage::new("https://www.googleapis.com", &bucket_name, service_account_key.clone())
        }))
        .ftps(ftps_certs_file, ftps_key_file)
        .listen(BIND_ADDRESS)
        .await?;
    } else {
        libunftp::Server::new(Box::new(move || {
            libunftp::storage::cloud_storage::CloudStorage::new("https://www.googleapis.com", &bucket_name, service_account_key.clone())
        }))
        .listen(BIND_ADDRESS)
        .await?;
    }

    Ok(())
}
