use clap::{arg, value_parser, Command};

#[derive(Debug)]
pub enum CliCommand {
    SetGamma { value: u8 },
    Enable,
    Disable,
    Status,
}

#[derive(Debug)]
pub struct CliArgs {
    pub command: CliCommand,
}

pub fn parse_args() -> CliArgs {
    let matches = Command::new("gd")
        .about("GammaDaemon CLI - manage screen brightness based on battery life")
        .arg(
            arg!(<Command> "Command to perform: set|enable|disable|status")
                .value_parser(["set", "enable", "disable", "status"]),
        )
        .arg(
            arg!([Value] "Gamma value (required when Action is set)")
                .value_parser(value_parser!(u8)),
        )
        .get_matches();

    let action = matches
        .get_one::<String>("Command")
        .map(|s| s.as_str())
        .unwrap_or_else(|| {
            eprintln!("No command provided. Use --help for usage information.");
            std::process::exit(1);
        });

    let command = match action {
        "set" => {
            let value = matches.get_one::<u8>("Value").copied().unwrap_or_else(|| {
                eprintln!("Command 'set' requires a Value. Usage: gd set <Value>");
                std::process::exit(1);
            });

            CliCommand::SetGamma { value: value }
        }
        "enable" => CliCommand::Enable,
        "disable" => CliCommand::Disable,
        "status" => CliCommand::Status,
        _ => unreachable!("Validated by clap value_parser"),
    };

    CliArgs { command }
}
