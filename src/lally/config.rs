use argh::FromArgs;

fn default_r_quorum() -> usize {
    1
}
fn default_w_quorum() -> usize {
    1
}
fn default_port() -> u32 {
    3000
}

#[derive(FromArgs)]
/// A simple in memory kv store trying its best to be available
pub struct Config {
    /// wipe previous wal log data and start anew...
    #[argh(switch)]
    fresh: bool,

    /// path to wal log for replay
    #[argh(option)]
    path: Option<String>,

    /// ipv4 address of seed node
    #[argh(option)]
    ip: Option<String>,

    /// custom port for server, default is 3000
    #[argh(option, default = "default_port()")]
    port: u32,

    /// read quorum value
    #[argh(option, default = "default_r_quorum()")]
    read_quorum: usize,

    /// read quorum value
    #[argh(option, default = "default_w_quorum()")]
    write_quorum: usize,
}

impl Config {
    pub fn new() -> Self {
        argh::from_env()
    }
    pub fn fresh(&self) -> bool {
        self.fresh
    }
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
    pub fn ip(&self) -> Option<&str> {
        self.ip.as_deref()
    }
    pub fn port(&self) -> u32 {
        self.port
    }
    pub fn read_quorum(&self) -> usize {
        self.read_quorum
    }
    pub fn write_quorum(&self) -> usize {
        self.write_quorum
    }
}
