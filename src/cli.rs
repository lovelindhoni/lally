use argh::FromArgs;

#[derive(FromArgs)]
/// A simple in memory kv store trying its best to be ACID compliant
pub struct Config {
    /// wipe previous wal log data and start anew...
    #[argh(switch)]
    pub fresh: bool,

    /// path to wal log for replay
    #[argh(option)]
    pub path: Option<String>,
}

pub fn config() -> Config {
    argh::from_env()
}
