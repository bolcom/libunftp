use clap::{App, Arg};
use std::{error::Error, result::Result};

const BUCKET_NAME: &str = "bucket-name";
const SERVICE_ACCOUNT_KEY: &str = "service-account-key";

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let matches = App::new("Example for using libunftp with Google Cloud Storage backend")
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
        .get_matches();

    let service_account_key = matches
        .value_of(SERVICE_ACCOUNT_KEY)
        .ok_or_else(|| "Internal error: use of an undefined command line parameter")?;
    let bucket_name = matches
        .value_of(BUCKET_NAME)
        .ok_or_else(|| "Internal error: use of an undefined command line parameter")?
        .to_owned();

    let service_account_key = yup_oauth2::read_service_account_key(service_account_key).await?;

    let server = libunftp::Server::new(Box::new(move || {
        libunftp::storage::cloud_storage::CloudStorage::new(&bucket_name, service_account_key.clone())
    }));

    server.listen("127.0.0.1:2121").await;
    Ok(())
}
