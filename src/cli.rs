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
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Check,
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
        Command::Mcp => {
            println!("mcp command scaffold active");
            Ok(())
        }
        Command::Status => {
            println!("status command scaffold active");
            Ok(())
        }
    }
}
