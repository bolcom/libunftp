use clap::{Arg, Command};
use libunftp::ServerBuilder;
use std::{error::Error, path::PathBuf};
use tracing::Level;
use unftp_sbe_gcs::options::AuthMethod;

// To run this example with the local fake GCS (see tests/resources/gcs_test.sh) instead of Google GCS,
// after starting fake-gcs-server, run this example with
//   --fake-gcs-base-url http://localhost:9081
//   --bucket-name test-bucket
//   --service-account-key test.json    # create test.json first with `echo unftp_test > test.json`

const BUCKET_NAME: &str = "bucket-name";
const SERVICE_ACCOUNT_KEY: &str = "service-account-key";
const FTPS_CERTS_FILE: &str = "ftps-certs-file";
const FTPS_KEY_FILE: &str = "ftps-key-file";
const BIND_ADDRESS: &str = "127.0.0.1:2121";
const FAKE_GCS_BASE_URL: &str = "fake-gcs-base-url";

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().with_max_level(Level::TRACE).init();

    let matches = Command::new("Example for using libunftp with Google Cloud Storage backend with optionally enabling TLS")
        .about("An FTP server that uses Google Cloud Storage as a backend")
        .author("The bol.com unFTP team")
        .arg(
            Arg::new(BUCKET_NAME)
                .short('b')
                .long(BUCKET_NAME)
                .value_name("BUCKET_NAME")
                .env("LIBUNFTP_BUCKET_NAME")
                .help("The name of the Google Cloud Storage bucket to be used")
                .required(true),
        )
        .arg(
            Arg::new(SERVICE_ACCOUNT_KEY)
                .short('s')
                .long(SERVICE_ACCOUNT_KEY)
                .value_name("SERVICE_ACCOUNT_KEY")
                .env("LIBUNFTP_SERVICE_ACCOUNT_KEY")
                .help("The service account key JSON file of the Google Cloud Storage bucket to be used")
                .required(false),
        )
        .arg(
            Arg::new(FAKE_GCS_BASE_URL)
                .short('u')
                .long(FAKE_GCS_BASE_URL)
                .value_name("GCS_BASE_URL")
                .env("LIBUNFTP_FAKE_GCS_BASE_URL")
                .help("Alternative GCS Base URL to use for testing.")
                .required(false),
        )
        .arg(
            Arg::new(FTPS_CERTS_FILE)
                .short('c')
                .long(FTPS_CERTS_FILE)
                .value_name("FTPS_CERTS_FILE")
                .env("LIBUNFTP_FTPS_CERTS_FILE")
                .help("The ftps certs file")
                .requires(FTPS_KEY_FILE),
        )
        .arg(
            Arg::new(FTPS_KEY_FILE)
                .short('p')
                .long(FTPS_KEY_FILE)
                .value_name("FTPS_KEY_FILE")
                .env("LIBUNFTP_FTPS_KEY_FILE")
                .help("The ftps certs key file")
                .requires(FTPS_CERTS_FILE),
        )
        .get_matches();

    let service_account_key_path = matches.get_one::<String>(SERVICE_ACCOUNT_KEY);
    let bucket_name = matches.get_one::<String>(BUCKET_NAME).unwrap().to_owned();
    let gcs_base_url = if let Some(base_url) = matches.get_one::<String>(FAKE_GCS_BASE_URL) {
        String::from(base_url)
    } else {
        String::from("https://www.googleapis.com")
    };

    let service_account_key: Option<Vec<u8>> = match service_account_key_path {
        Some(key_path) => Some(tokio::fs::read(key_path).await?),
        None => None,
    };

    let mut builder = ServerBuilder::new(Box::new(move || match &service_account_key {
        Some(key) => unftp_sbe_gcs::CloudStorage::with_api_base(&gcs_base_url, &bucket_name, PathBuf::new(), key.clone()),
        None => unftp_sbe_gcs::CloudStorage::with_api_base(&gcs_base_url, &bucket_name, PathBuf::new(), AuthMethod::None),
    }));

    builder = if let Some(ftps_certs_file) = matches.get_one::<String>(FTPS_CERTS_FILE) {
        let ftps_key_file = matches.get_one::<String>(FTPS_KEY_FILE).unwrap();
        builder.ftps(ftps_certs_file, ftps_key_file)
    } else {
        builder
    };

    builder.build().unwrap().listen(BIND_ADDRESS).await?;

    Ok(())
}
