#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    New,
    Open,
    Close,
    Quit,
    List,
    Search(String),
    Site(String),
    Number(usize),
    Title(String),
    SiteTitle(String, String),
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    if values.iter().any(|s| s.trim().is_empty()) {
        return Err("query must not be empty".into());
    }
    match values.as_slice() {
        [] | ["new"] => Ok(Command::New),
        ["help" | "--help" | "-h"] => Ok(Command::Help),
        ["open"] => Ok(Command::Open),
        ["close"] => Ok(Command::Close),
        ["quit"] => Ok(Command::Quit),
        ["list"] => Ok(Command::List),
        ["search", site] => Ok(Command::Site((*site).into())),
        ["tab", number] if number.chars().all(|c| c.is_ascii_digit()) => number
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .map(Command::Number)
            .ok_or_else(|| "tab number must be a positive integer".into()),
        ["tab", title] if !title.starts_with('-') => Ok(Command::Title((*title).into())),
        ["tab", site, title] => Ok(Command::SiteTitle((*site).into(), (*title).into())),
        ["--", query @ ..] if !query.is_empty() => Ok(Command::Search(query.join(" "))),
        [
            "new" | "open" | "close" | "quit" | "list" | "help" | "tab" | "search" | "--",
            ..,
        ] => Err("invalid arguments; run hel --help".into()),
        [first, ..] if first.starts_with('-') => Err("unknown option; run hel --help".into()),
        _ => Ok(Command::Search(values.join(" "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_browser_commands_and_queries() {
        for (input, expected) in [
            (vec![], Command::New),
            (vec!["new"], Command::New),
            (vec!["open"], Command::Open),
            (vec!["close"], Command::Close),
            (vec!["quit"], Command::Quit),
            (vec!["list"], Command::List),
            (vec!["--help"], Command::Help),
            (
                vec!["rust ownership"],
                Command::Search("rust ownership".into()),
            ),
            (
                vec!["rust", "ownership"],
                Command::Search("rust ownership".into()),
            ),
            (vec!["--", "close"], Command::Search("close".into())),
            (vec!["search", "youtube"], Command::Site("youtube".into())),
            (vec!["tab", "2"], Command::Number(2)),
            (vec!["tab", "Rust book"], Command::Title("Rust book".into())),
            (
                vec!["tab", "youtube", "Rust"],
                Command::SiteTitle("youtube".into(), "Rust".into()),
            ),
        ] {
            assert_eq!(parse(&args(&input)).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_input_never_becomes_a_browser_mutation() {
        for input in [
            vec![""],
            vec!["  "],
            vec!["--"],
            vec!["--bad"],
            vec!["tab"],
            vec!["tab", "0"],
            vec!["tab", "-1"],
            vec!["tab", "9999999999999999999999999999999999999"],
            vec!["tab", ""],
            vec!["tab", "youtube", ""],
            vec!["tab", "a", "b", "c"],
            vec!["search"],
            vec!["search", ""],
            vec!["search", "a", "b"],
            vec!["new", "a"],
            vec!["quit", "a"],
        ] {
            assert!(parse(&args(&input)).is_err(), "{input:?}");
        }
    }
}
