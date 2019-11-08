macro_rules! spawn {
    ($future:expr) => {
        tokio::spawn($future.map(|_| ()).map_err(|_| ()));
    };
}
