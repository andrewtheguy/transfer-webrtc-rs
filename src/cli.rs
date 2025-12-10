use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sendfile")]
#[command(author, version, about = "P2P file transfer using WebRTC", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// PeerJS server hostname
    #[arg(short, long, default_value = "0.peerjs.com")]
    pub server: String,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Send a file to a peer
    Send {
        /// Path to the file to send
        file: PathBuf,

        /// Specify your peer ID (optional, will generate one if not provided)
        #[arg(short, long)]
        peer_id: Option<String>,
    },

    /// Receive a file from a peer
    Receive {
        /// Peer ID of the sender
        peer_id: String,

        /// Output directory (default: current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
