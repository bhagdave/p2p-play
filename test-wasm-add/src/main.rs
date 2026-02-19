fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: add <int1> <int2>");
        std::process::exit(1);
    }
    let a: i64 = args[1].parse().unwrap_or_else(|e| {
        eprintln!("Invalid first argument: {}", e);
        std::process::exit(1);
    });
    let b: i64 = args[2].parse().unwrap_or_else(|e| {
        eprintln!("Invalid second argument: {}", e);
        std::process::exit(1);
    });
    println!("{}", a + b);
}
