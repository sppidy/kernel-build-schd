use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "kbs", about = "Kernel build scheduler")]
pub struct Args {
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
    match args.command {
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            println!("configuration command scaffold active");
            Ok(())
        }
        Command::Daemon { foreground: _ } => {
            println!("daemon command scaffold active");
            Ok(())
        }
        Command::Mcp => crate::mcp::serve_stdio().await,
        Command::Status => {
            println!("status command scaffold active");
            Ok(())
        }
        Command::Jobs { command: _ } => {
            println!("jobs command scaffold active");
            Ok(())
        }
        Command::Logs { command: _ } => {
            println!("logs command scaffold active");
            Ok(())
        }
    }
}
