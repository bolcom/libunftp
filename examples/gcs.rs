use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";

    let server = libunftp::Server::new(Box::new(move || {
        libunftp::storage::cloud_storage::CloudStorage::new("your-bucket-name", Tp {})
    }));

    info!("Starting ftp server on {}", addr);
    server.listen(addr);
}

struct Tp {}

impl libunftp::storage::cloud_storage::TokenProvider for Tp {
    fn get_token(
        &self,
    ) -> std::result::Result<libunftp::storage::cloud_storage::Token, Box<dyn std::error::Error>>
    {
        let (token_type, access_token) = sync_oauth2::get_token(
            sync_oauth2::yup_oauth2::service_account_key_from_file(
                &"/path/to/your/service/account/key.json".to_string(),
            )
            .expect("borked"),
        )?;
        Ok(libunftp::storage::cloud_storage::Token {
            token_type,
            access_token,
        })
    }
}
