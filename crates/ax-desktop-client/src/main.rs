//! ax-desktop — native wgpu Command Center (egui/eframe).

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "ax-desktop",
    version,
    about = "Native wgpu Command Center for ax"
)]
struct Args {
    /// Project root (must contain `.ax/ax.db`). Defaults to current directory.
    path: Option<String>,

    /// Port for the embedded ax-web server (default 7070).
    #[arg(long, default_value_t = 7070)]
    port: u16,

    /// Bind address for the embedded server.
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

fn main() {
    let args = Args::parse();
    if let Err(e) = ax_desktop_client::run(args.path, args.port, args.bind) {
        eprintln!("ax-desktop: {e}");
        std::process::exit(1);
    }
}
