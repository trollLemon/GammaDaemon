mod args;
mod commands;

use gamma_daemon::constants;

fn main() {
    let cli_args = args::parse_args();

    let result = match cli_args.command {
        args::CliCommand::SetGamma { value } => commands::set_gamma(value),
        args::CliCommand::Enable => commands::enable_gamma_daemon(constants::DEFAULT_SOCKET_PATH),
        args::CliCommand::Disable => commands::disable_gamma_daemon(constants::DEFAULT_SOCKET_PATH),
        args::CliCommand::Status => commands::show_status(constants::DEFAULT_SOCKET_PATH),
    };

    match result {
        Ok(message) => {
            println!("{}", message);
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
