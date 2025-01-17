use anyhow::{bail, Context, Result};
use argh::FromArgs;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{canonicalize, copy, create_dir_all, read_to_string, OpenOptions};
use tracing::{debug, info};

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
    ip: Option<String>,

    /// custom port for server, default is 3000
    #[argh(option)]
    port: Option<u32>,

    /// read quorum value
    #[argh(option)]
    read_quorum: Option<usize>,

    /// write quorum value
    #[argh(option)]
    write_quorum: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    fresh: bool,

    #[serde(default)]
    replay_log: Option<PathBuf>,

    ip: Option<String>,

    #[serde(default = "default_port")]
    port: u32,

    #[serde(default = "default_r_quorum")]
    read_quorum: usize,

    #[serde(default = "default_w_quorum")]
    write_quorum: usize,

    #[serde(skip)]
    aof_storage_path: PathBuf,
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
        if let Some(ip) = cli_args.ip {
            info!("Seed node IP: {}", ip);
            config.ip = Some(ip);
        }
        if let Some(port) = cli_args.port {
            config.port = port;
            info!("Server port set to: {}", port);
        }
        if let Some(read_quorum) = cli_args.read_quorum {
            config.read_quorum = read_quorum;
            info!("Read quorum set to: {}", read_quorum);
        }
        if let Some(write_quorum) = cli_args.write_quorum {
            config.write_quorum = write_quorum;
            info!("Write quorum set to: {}", write_quorum);
        }

        // Initialize project directories and handle log file
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

        // If fresh start is requested, handle accordingly
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

        // Handle replay log copy if provided
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
    pub fn aof_file(&self) -> &Path {
        &self.aof_storage_path
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fresh: false,
            replay_log: None,
            ip: None,
            port: default_port(),
            read_quorum: default_r_quorum(),
            write_quorum: default_w_quorum(),
            aof_storage_path: PathBuf::new(), // Will be initialized properly in new()
        }
    }
}
