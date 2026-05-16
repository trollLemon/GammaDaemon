use clap::{arg, value_parser, Command};

#[derive(Debug)]
pub enum CliCommand {
    SetGamma { value: f32 },
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
            arg!([Value] "Gamma fraction in [0.0, 1.0] (required when Action is set)")
                .value_parser(value_parser!(f32)),
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
            let value = matches.get_one::<f32>("Value").copied().unwrap_or_else(|| {
                eprintln!("Command 'set' requires a Value. Usage: gd set <Value>");
                std::process::exit(1);
            });

            if !(0.0..=1.0).contains(&value) {
                eprintln!("Gamma value must be in the range [0.0, 1.0]");
                std::process::exit(1);
            }

            CliCommand::SetGamma { value }
        }
        "enable" => CliCommand::Enable,
        "disable" => CliCommand::Disable,
        "status" => CliCommand::Status,
        _ => unreachable!("Validated by clap value_parser"),
    };

    CliArgs { command }
}
