//! Command-line entry point for Lanyard.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "lanyard-ssh-agent", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the SSH agent proxy.
    Serve,
    /// Register an upstream SSH agent socket.
    Register { socket: PathBuf },
    /// Remove a previously registered upstream socket.
    Unregister { socket: PathBuf },
    /// Show the daemon and upstream-agent state.
    Status {
        /// Emit stable JSON suitable for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Print a Lanyard socket path.
    Socket {
        /// Print the control socket instead of the SSH agent socket.
        #[arg(long)]
        control: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve => println!("serve is not implemented yet"),
        Command::Register { socket } => println!("register {}", socket.display()),
        Command::Unregister { socket } => println!("unregister {}", socket.display()),
        Command::Status { json } => println!("status json={json}"),
        Command::Socket { control } => println!("socket control={control}"),
    }
}
