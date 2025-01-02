use argh::FromArgs;

#[derive(FromArgs)]
/// A simple in memory kv store trying its best to be available
pub struct Config {
    /// wipe previous wal log data and start anew...
    #[argh(switch)]
    pub fresh: bool,

    /// path to wal log for replay
    #[argh(option)]
    pub path: Option<String>,

    /// ipv4 address of seed node
    #[argh(option)]
    pub ip: Option<String>,

    /// custom port for server, default is 3000
    #[argh(option)]
    pub port: Option<u32>,
}

pub fn config() -> Config {
    argh::from_env()
}
