pub fn get_token(
    service_account_key: yup_oauth2::ServiceAccountKey,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    use hyper::{net::HttpsConnector, Client};
    use hyper_rustls::TlsClient;
    use yup_oauth2::{self, GetToken, ServiceAccountAccess};

    let token = ServiceAccountAccess::new(
        service_account_key,
        Client::with_connector(HttpsConnector::new(TlsClient::new())),
    )
    .token(vec![
        &"https://www.googleapis.com/auth/devstorage.read_write",
    ])?;

    Ok((token.token_type, token.access_token))
}

pub use yup_oauth2;
