use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{
    config::{load_config, Config},
    control::{ControlClient, ControlRequest, ControlResponse},
};

const DEFAULT_CONFIG: &str = "/etc/kernel-build-scheduler/config.toml";
const USER_CONFIG_RELATIVE: &str = ".config/kernel-build-scheduler/config.toml";

#[derive(Debug, Parser)]
#[command(name = "kbs", about = "Kernel build scheduler")]
pub struct Args {
    #[arg(long, env = "KBS_CONFIG", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Daemon {
        #[arg(long)]
        foreground: bool,
    },
    Mcp,
    Status,
    Jobs {
        #[command(subcommand)]
        command: JobsCommand,
    },
    Logs {
        #[command(subcommand)]
        command: LogsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Check,
}

#[derive(Debug, Subcommand)]
pub enum JobsCommand {
    List,
    Show { id: String },
    Cancel { id: String },
}

#[derive(Debug, Subcommand)]
pub enum LogsCommand {
    Tail { id: String },
}

pub async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = load_config_for_cli(args.config)?;

    match args.command {
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            config.validate()?;
            println!("configuration ok");
            Ok(())
        }
        Command::Daemon { foreground: _ } => {
            crate::daemon::run_foreground_with_config(config).await
        }
        Command::Mcp => {
            if !config.mcp.stdio_enabled {
                anyhow::bail!("mcp stdio is disabled in config");
            }
            crate::mcp::serve_stdio(config.mcp.control_socket.as_std_path()).await
        }
        Command::Status => print_control_response(&config, ControlRequest::Status).await,
        Command::Jobs {
            command: JobsCommand::List,
        } => print_control_response(&config, ControlRequest::ListJobs).await,
        Command::Jobs {
            command: JobsCommand::Show { id },
        } => print_control_response(&config, ControlRequest::GetJob { id: id.parse()? }).await,
        Command::Jobs {
            command: JobsCommand::Cancel { id },
        } => print_control_response(&config, ControlRequest::Cancel { id: id.parse()? }).await,
        Command::Logs {
            command: LogsCommand::Tail { id },
        } => {
            print_control_response(
                &config,
                ControlRequest::TailLog {
                    id: id.parse()?,
                    max_bytes: config.security.max_log_read_bytes,
                },
            )
            .await
        }
    }
}

fn load_config_for_cli(path: Option<PathBuf>) -> anyhow::Result<Config> {
    if let Some(path) = path {
        return Ok(load_config(path)?);
    }

    let system = PathBuf::from(DEFAULT_CONFIG);
    if system.exists() {
        return Ok(load_config(system)?);
    }

    if let Some(home) = std::env::var_os("HOME") {
        let user = PathBuf::from(home).join(USER_CONFIG_RELATIVE);
        if user.exists() {
            return Ok(load_config(user)?);
        }
    }

    anyhow::bail!("no config found; pass --config or create {DEFAULT_CONFIG}")
}

async fn print_control_response(config: &Config, request: ControlRequest) -> anyhow::Result<()> {
    let response = request_control(config, request).await?;
    println!("{}", format_control_response(&response)?);
    Ok(())
}

async fn request_control(
    config: &Config,
    request: ControlRequest,
) -> anyhow::Result<ControlResponse> {
    let client = ControlClient::connect(config.mcp.control_socket.as_std_path()).await?;
    Ok(client.request(request).await?)
}

fn format_control_response(response: &ControlResponse) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(response)?)
}
