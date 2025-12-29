use crate::auth::UserDetail;
use crate::server::session::SharedSession;
use crate::storage::StorageBackend;
use dashmap::{DashMap, Entry};
use std::net::{IpAddr, SocketAddr};
use std::ops::RangeInclusive;

/// Identifies a passive listening port entry in the Switchboard that is associated with a specific
/// session. The key is constructed out of the external source IP of the client and the passive listening port that has
/// been reserved for the client via the 'PASV' command
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) struct SwitchboardKey {
    source: IpAddr,
    port: u16,
}

impl SwitchboardKey {
    fn new(source: IpAddr, port: u16) -> Self {
        SwitchboardKey { source, port }
    }
}

impl From<&SocketAddrPair> for SwitchboardKey {
    fn from(connection: &SocketAddrPair) -> Self {
        SwitchboardKey::new(connection.source.ip(), connection.destination.port())
    }
}

/// Connect clients to the right data channel
#[derive(Debug)]
pub(in crate::server) struct Switchboard<S, U>
where
    S: StorageBackend<U>,
    U: UserDetail,
{
    switchboard: DashMap<SwitchboardKey, Option<SharedSession<S, U>>>,
    port_range: RangeInclusive<u16>,
    logger: slog::Logger,
}

#[derive(Debug)]
pub(in crate::server) enum SwitchboardError {
    // SwitchBoardNotInitialized,
    EntryNotAvailable,
    // EntryCreationFailed,
    MaxRetriesError,
}

impl<S, U> Switchboard<S, U>
where
    S: StorageBackend<U>,
    U: UserDetail + 'static,
{
    pub fn new(logger: slog::Logger, passive_ports: RangeInclusive<u16>) -> Self {
        let board = DashMap::new();
        Self {
            switchboard: board,
            port_range: passive_ports,
            logger,
        }
    }

    pub async fn try_and_claim(&mut self, key: SwitchboardKey, session_arc: SharedSession<S, U>) -> Result<(), SwitchboardError> {
        // Atomically insert the key and value into the switchboard hashmap
        match self.switchboard.entry(key) {
            Entry::Occupied(_) => Err(SwitchboardError::EntryNotAvailable),
            Entry::Vacant(entry) => {
                entry.insert(Some(session_arc));
                Ok(())
            }
        }
    }

    pub fn unregister_by_connection_pair(&mut self, connection: &SocketAddrPair) {
        let hash = connection.into();

        self.unregister_by_key(&hash)
    }

    pub fn unregister_by_key(&mut self, key: &SwitchboardKey) {
        if self.switchboard.remove(key).is_none() {
            slog::warn!(self.logger, "Entry already removed? key: {:?}", key);
        }
    }

    #[tracing_attributes::instrument]
    pub async fn get_session_by_connection_pair(&mut self, connection: &SocketAddrPair) -> Option<SharedSession<S, U>> {
        let key: SwitchboardKey = connection.into();

        match self.switchboard.get(&key) {
            Some(session) => session.clone(),
            None => None,
        }
    }

    /// Find the next available port within the specified range (inclusive of the upper limit).
    /// The reserved port is associated with the source ip of the client and the associated session, using a hashmap
    ///
    //#[tracing_attributes::instrument]
    pub async fn reserve(&mut self, session_arc: SharedSession<S, U>) -> Result<u16, SwitchboardError> {
        let range_size = self.port_range.end() - self.port_range.start();

        let randomized_initial_port = {
            let mut data = [0; 2];
            getrandom::fill(&mut data).expect("Error generating random free port to reserve");
            u16::from_ne_bytes(data)
        };

        // Claims the next available listening port
        // The search starts at randomized_initial_port.
        // If a port is already claimed, the loop continues to the next port until an available port is found.
        // The function returns the first available port it finds or an error if no ports are available.
        let mut session = session_arc.lock().await;
        let control_connection = session
            .control_connection
            .expect("BUG: reserve() called on a session with no control_connection details");
        for i in 0..=range_size {
            let port = self.port_range.start() + ((randomized_initial_port + i) % range_size);
            slog::debug!(self.logger, "Trying if port {} is available", port);
            let key = SwitchboardKey::new(control_connection.source.ip(), port);

            match &self.try_and_claim(key.clone(), session_arc.clone()).await {
                Ok(_) => {
                    // Remove and disassociate existing passive channels
                    if let Some(active_datachan_key) = &session.switchboard_active_datachan {
                        if active_datachan_key != &key {
                            slog::info!(self.logger, "Removing stale session data channel {:?}", &active_datachan_key);
                            self.unregister_by_key(active_datachan_key);
                        }
                    }

                    // Associate the new port with the session,
                    session.switchboard_active_datachan = Some(key);
                    return Ok(port);
                }
                Err(_) => {
                    slog::debug!(self.logger, "Port entry is occupied (key: {:?}), trying to find a vacant one", &key);
                    continue;
                }
            }
        }

        slog::warn!(self.logger, "Out of tries reserving next free port!");
        Err(SwitchboardError::MaxRetriesError)
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct SocketAddrPair {
    pub source: SocketAddr,
    pub destination: SocketAddr,
}
