use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "quickstatic")]
#[command(author = "APIToolkit. <hello@apitoolkit.io>")]
#[command(version = "1.0")]
#[command(about = "Simple & Fast Static site engine with support for the extended markdown", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Sets the log level to be used. Eg trace, debug, warn, info, error
    #[arg(short, long, default_value = "info")]
    pub log_level: String,

    /// Sets the YAML test configuration file
    #[arg(short, long, default_value = ".")]
    pub dir: String,
}

#[derive(Subcommand)]
pub enum Commands {
    Build {},
    Serve {
        /// Sets the YAML test configuration file
        #[arg(short, long, default_value = "2020")]
        port: u16,
    },
}
