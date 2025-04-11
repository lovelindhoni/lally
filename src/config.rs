use anyhow::{bail, Context, Result};
use argh::FromArgs;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{canonicalize, copy, create_dir_all, read_to_string, OpenOptions};
use tracing::{debug, info, warn};

#[inline]
fn default_r_quorum() -> usize {
    1
}

#[inline]
fn default_w_quorum() -> usize {
    1
}

#[inline]
fn default_http_port() -> u16 {
    3000
}

#[inline]
fn default_grpc_port() -> u16 {
    50071
}

#[inline]
fn default_aof_flush_interval() -> u64 {
    100
}

#[derive(FromArgs)]
/// A simple in memory kv store trying its best to be available
pub struct CliArgs {
    /// path to config file (lally.yml)
    #[argh(option)]
    config: Option<PathBuf>,

    /// wipe previous wal log data and start anew...
    #[argh(switch)]
    fresh: Option<bool>,

    /// path to aof log for replay
    #[argh(option)]
    replay_log: Option<PathBuf>,

    /// ipv4 address of seed node
    #[argh(option)]
    seed_node: Option<String>,

    /// custom port for http server, default is 3000
    #[argh(option)]
    http_port: Option<u16>,

    /// custom port for grpc server, default is 50071
    #[argh(option)]
    grpc_port: Option<u16>,

    /// read quorum value
    #[argh(option)]
    read_quorum: Option<usize>,

    /// write quorum value
    #[argh(option)]
    write_quorum: Option<usize>,

    /// aof flush interval in milliseconds
    #[argh(option)]
    aof_flush_interval: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    fresh: bool,

    #[serde(default)]
    replay_log: Option<PathBuf>,

    seed_node: Option<String>,

    #[serde(default = "default_http_port")]
    http_port: u16,

    #[serde(default = "default_grpc_port")]
    grpc_port: u16,

    #[serde(default = "default_r_quorum")]
    read_quorum: usize,

    #[serde(default = "default_w_quorum")]
    write_quorum: usize,

    #[serde(skip)]
    aof_storage_path: PathBuf,

    #[serde(default = "default_aof_flush_interval")]
    aof_flush_interval: u64,
}

impl Config {
    pub async fn new() -> Result<Self> {
        let cli_args: CliArgs = argh::from_env();

        let mut config = Self::load_config_file(&cli_args.config)
            .await
            .context("Failed to load config file")?;

        // Override with CLI arguments if present
        if let Some(fresh) = cli_args.fresh {
            config.fresh = fresh;
            info!("Fresh start requested.");
        }
        if let Some(path) = cli_args.replay_log {
            info!("Replay log file set: {:?}", path);
            config.replay_log = Some(path);
        }
        if let Some(addr) = cli_args.seed_node {
            info!("Seed node address: {}", addr);
            config.seed_node = Some(addr);
        }
        if let Some(http_port) = cli_args.http_port {
            config.http_port = http_port;
            info!("HTTP server port set to: {}", http_port);
        }
        if let Some(aof_flush_interval) = cli_args.aof_flush_interval {
            config.aof_flush_interval = aof_flush_interval;
            info!(
                "Append only log flush interval set to: {}",
                aof_flush_interval
            );
        }
        if let Some(grpc_port) = cli_args.grpc_port {
            config.grpc_port = grpc_port;
            info!("GRPC server port set to: {}", grpc_port);
            warn!("This assumes that all nodes in the cluster will use the same port for the gRPC server.");
        }
        if let Some(read_quorum) = cli_args.read_quorum {
            config.read_quorum = read_quorum;
            info!("Read quorum set to: {}", read_quorum);
        }
        if let Some(write_quorum) = cli_args.write_quorum {
            config.write_quorum = write_quorum;
            info!("Write quorum set to: {}", write_quorum);
        }

        config.initialize_log_file().await?;

        debug!("Final configuration: {:?}", config);

        Ok(config)
    }

    async fn initialize_log_file(&mut self) -> Result<()> {
        // Get project directory
        let project_dirs = ProjectDirs::from("com", "Lally", "Lally")
            .context("Could not find project directories")?;

        info!("Found project directory at: {:?}", project_dirs.data_dir());

        create_dir_all(project_dirs.data_dir())
            .await
            .context("Failed to create data directory")?;

        let mut aof_storage_path = project_dirs.data_dir().to_path_buf();
        aof_storage_path.push("aof.txt");
        self.aof_storage_path = aof_storage_path;

        if self.fresh && self.replay_log.is_some() {
            bail!("Don't specify replay log file when starting fresh");
        }

        // Handle fresh start
        if self.fresh {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.aof_storage_path)
                .await
                .context("Failed to create fresh AOF file")?;
            info!("Created fresh AOF file at: {:?}", self.aof_storage_path);
            return Ok(());
        }

        // Handle replay log copy to the fixed location if provided
        if let Some(source_path) = &self.replay_log {
            let canonical_source = canonicalize(source_path)
                .await
                .context("Failed to canonicalize replay log path")?;

            copy(&canonical_source, &self.aof_storage_path)
                .await
                .context("Failed to copy AOF file")?;

            info!(
                "Successfully copied AOF log from {:?} to {:?}",
                source_path, self.aof_storage_path
            );
        }

        Ok(())
    }

    async fn load_config_file(config_path: &Option<PathBuf>) -> Result<Self> {
        let config_path = if let Some(path) = config_path {
            path.clone()
        } else {
            let default_path = Path::new("lally.yml");
            if default_path.exists() {
                default_path.to_path_buf()
            } else {
                // Return default config if no file exists
                return Ok(Self::default());
            }
        };

        if !config_path.exists() {
            bail!("Config file doesn't exist: {}", config_path.display());
        }

        info!("Loading config file from: {:?}", config_path);

        let contents = read_to_string(&config_path)
            .await
            .context("Failed to read config file")?;

        serde_yaml::from_str(&contents).context("Failed to parse config file")
    }

    // getters
    pub fn seed_node(&self) -> Option<&str> {
        self.seed_node.as_deref()
    }
    pub fn http_port(&self) -> u16 {
        self.http_port
    }
    pub fn grpc_port(&self) -> u16 {
        self.grpc_port
    }
    pub fn read_quorum(&self) -> usize {
        self.read_quorum
    }
    pub fn write_quorum(&self) -> usize {
        self.write_quorum
    }
    pub fn aof_file(&self) -> &Path {
        &self.aof_storage_path
    }
    pub fn aof_flush_interval(&self) -> u64 {
        self.aof_flush_interval
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fresh: false,
            replay_log: None,
            seed_node: None,
            http_port: default_http_port(),
            grpc_port: default_grpc_port(),
            read_quorum: default_r_quorum(),
            write_quorum: default_w_quorum(),
            aof_flush_interval: default_aof_flush_interval(),
            aof_storage_path: PathBuf::new(), // Will be initialized properly in new()
        }
    }
}
