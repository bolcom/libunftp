//! Represents the chosen options that the libunftp user opted for.

use crate::notification::{DataListener, PresenceListener};
use crate::options::ActivePassiveMode;
use crate::{
    auth::AuthenticationPipeline,
    options::{FtpsRequired, PassiveHost, SiteMd5},
    server::controlchan,
    server::tls::FtpsConfig,
};
use std::ops::RangeInclusive;
use std::{sync::Arc, time::Duration};
use unftp_core::auth::{Authenticator, UserDetail, UserDetailProvider};
use unftp_core::storage::{Metadata, StorageBackend};

// Holds the options the libunftp user opted for.
pub struct OptionsHolder<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub storage: Arc<dyn (Fn() -> Storage) + Send + Sync>,
    pub greeting: &'static str,
    pub authenticator: Arc<dyn Authenticator>,
    pub user_detail_provider: Arc<dyn UserDetailProvider<User = User> + Send + Sync>,
    pub passive_ports: RangeInclusive<u16>,
    pub passive_host: PassiveHost,
    pub ftps_config: FtpsConfig,
    pub collect_metrics: bool,
    pub idle_session_timeout: Duration,
    pub logger: slog::Logger,
    pub ftps_required_control_chan: FtpsRequired,
    pub ftps_required_data_chan: FtpsRequired,
    pub site_md5: SiteMd5,
    pub data_listener: Arc<dyn DataListener>,
    pub presence_listener: Arc<dyn PresenceListener>,
    pub active_passive_mode: ActivePassiveMode,
    pub binder: Arc<std::sync::Mutex<Option<Box<dyn crate::options::Binder>>>>,
}

impl<Storage, User> From<&OptionsHolder<Storage, User>> for controlchan::LoopConfig<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    fn from(server: &OptionsHolder<Storage, User>) -> Self {
        // So this is when you create a new storage backend?
        // XXX Shouldn't instantiate storage until _after_ successful auth.
        // Build the authentication pipeline from authenticator and user_detail_provider
        let auth_pipeline = Arc::new(AuthenticationPipeline::new(server.authenticator.clone(), server.user_detail_provider.clone()));

        controlchan::LoopConfig {
            auth_pipeline,
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
            data_listener: server.data_listener.clone(),
            presence_listener: server.presence_listener.clone(),
            active_passive_mode: server.active_passive_mode,
            binder: server.binder.clone(),
        }
    }
}
