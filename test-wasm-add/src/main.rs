use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: add <int1> <int2>");
        return ExitCode::FAILURE;
    }
    let a: i64 = match args[1].parse::<i64>() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid first argument: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let b: i64 = match args[2].parse::<i64>() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid second argument: {}", e);
            return ExitCode::FAILURE;
        }
    };
    println!("{}", a + b);
    ExitCode::SUCCESS
}
