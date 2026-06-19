mod manifest;
mod apply;
mod get;
mod delete;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "oabctl", about = "OAB agent provisioner for ECS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create or update OAB services from manifest files
    Apply {
        /// Path to manifest file or directory
        #[arg(short, long)]
        file: String,
    },
    /// List OAB services and their status
    Get {
        /// Resource type
        resource: String,
        /// Optional resource name
        name: Option<String>,
        /// ECS cluster name
        #[arg(long, default_value = "default")]
        cluster: String,
    },
    /// Delete an OAB service
    Delete {
        /// Resource type
        resource: String,
        /// Resource name
        name: String,
        /// ECS cluster name
        #[arg(long, default_value = "default")]
        cluster: String,
        /// Namespace
        #[arg(long, default_value = "prod")]
        namespace: String,
    },
    /// Execute a command in an agent container (via ecsctl)
    Exec {
        /// Agent name (alias)
        agent: String,
        /// Command to run
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Copy files to/from agent containers (via ecsctl)
    Cp {
        /// Source path (local or agent:/path)
        src: String,
        /// Destination path (local or agent:/path)
        dst: String,
    },
    /// Sync a local directory to an agent container (via ecsctl)
    Sync {
        /// Source directory
        src: String,
        /// Destination (agent:/path)
        dst: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

    match cli.command {
        Commands::Apply { file } => apply::run(&config, &file).await,
        Commands::Get { resource, name, cluster } => get::run(&config, &resource, name.as_deref(), &cluster).await,
        Commands::Delete { resource, name, cluster, namespace } => {
            delete::run(&config, &resource, &name, &cluster, &namespace).await
        }
        // TODO: Wire up ecsctl library once its API is refactored for library use.
        // Blocked on: oablab/ecsctl library API readiness (functions currently
        // shell out to `aws` CLI and print directly to stderr).
        Commands::Exec { agent, command } => {
            anyhow::bail!("exec not yet implemented — pending ecsctl library API refactor")
        }
        Commands::Cp { src, dst } => {
            anyhow::bail!("cp not yet implemented — pending ecsctl library API refactor")
        }
        Commands::Sync { src, dst } => {
            anyhow::bail!("sync not yet implemented — pending ecsctl library API refactor")
        }
    }
}
