use super::ServerError;
use crate::storage::Metadata;
use crate::{
    auth::Authenticator,
    auth::UserDetail,
    options::{FtpsRequired, PassiveHost, SiteMd5},
    server::controlchan,
    server::tls::FtpsConfig,
    storage::StorageBackend,
};
use std::{net::SocketAddr, ops::Range, sync::Arc, time::Duration};
use tokio::net::TcpListener;

pub struct GlobalOptions<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub storage: Arc<Box<dyn (Fn() -> Storage) + Send + Sync>>,
    pub greeting: &'static str,
    pub authenticator: Arc<dyn Authenticator<User>>,
    pub passive_ports: Range<u16>,
    pub passive_host: PassiveHost,
    pub ftps_config: FtpsConfig,
    pub collect_metrics: bool,
    pub idle_session_timeout: Duration,
    pub logger: slog::Logger,
    pub ftps_required_control_chan: FtpsRequired,
    pub ftps_required_data_chan: FtpsRequired,
    pub site_md5: SiteMd5,
}

impl<Storage, User> From<&GlobalOptions<Storage, User>> for controlchan::LoopConfig<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    fn from(server: &GlobalOptions<Storage, User>) -> Self {
        controlchan::LoopConfig {
            authenticator: server.authenticator.clone(),
            storage: (server.storage)(),
            ftps_config: server.ftps_config.clone(),
            collect_metrics: server.collect_metrics,
            greeting: server.greeting,
            idle_session_timeout: server.idle_session_timeout,
            passive_ports: server.passive_ports.clone(),
            passive_host: server.passive_host.clone(),
            logger: server.logger.new(slog::o!()),
            ftps_required_control_chan: server.ftps_required_control_chan,
            ftps_required_data_chan: server.ftps_required_data_chan,
            site_md5: server.site_md5,
        }
    }
}

pub struct Listener<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub bind_address: SocketAddr,
    pub logger: slog::Logger,
    pub options: GlobalOptions<Storage, User>,
}

impl<Storage, User> Listener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    pub async fn listen(self) -> std::result::Result<(), ServerError> {
        let Listener { logger, bind_address, options } = self;
        let listener = TcpListener::bind(bind_address).await?;
        loop {
            match listener.accept().await {
                Ok((tcp_stream, socket_addr)) => {
                    slog::info!(logger, "Incoming control connection from {:?}", socket_addr);
                    let result = controlchan::spawn_loop::<Storage, User>((&options).into(), tcp_stream, None, None).await;
                    if let Err(err) = result {
                        slog::error!(logger, "Could not spawn control channel loop for connection from {:?}: {:?}", socket_addr, err)
                    }
                }
                Err(err) => {
                    slog::error!(logger, "Error accepting incoming control connection {:?}", err);
                }
            }
        }
    }
}
