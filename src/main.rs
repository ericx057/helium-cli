mod app;
mod browser;
mod cli;
mod search;
mod state;

fn execute() -> Result<(), String> {
    let command = cli::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if command == cli::Command::Help {
        println!("{}", app::HELP);
        return Ok(());
    }
    let mut store = state::Store::open()?;
    let result = app::run(command, &browser::Browser, &mut store.state);
    store.save()?;
    println!("{}", result?);
    Ok(())
}

fn main() {
    if let Err(error) = execute() {
        eprintln!("hel: {error}");
        std::process::exit(1);
    }
}
