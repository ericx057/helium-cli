use crate::browser::{Browser, Created, Snapshot};
use crate::cli::Command;
use crate::search::{Index, Tab};
use crate::state::{State, TabKey};

pub const HELP: &str = "hel [\"search text\"]        Open a new search tab (DuckDuckGo)
hel new                     Open a blank tab (also: hel)
hel open                    Open a new Helium window
hel close                   Close the last tab created by hel
hel quit                    Close the last window created by hel open
hel list                    List all tabs by number
hel tab NUMBER              Focus a numbered tab and its window
hel tab \"TITLE\"             Focus a unique title match, or list matches
hel tab \"SITE\" \"TITLE\"      List tabs matching both site and title
hel search \"SITE\"           List tabs matching a hostname
hel -- \"close\"              Search for a reserved command word
hel --help                  Show this help";

pub trait BrowserApi {
    fn snapshot(&self) -> Result<Snapshot, String>;
    fn create(&self, url: Option<&str>, window: bool) -> Result<Created, String>;
    fn focus(&self, session: &str, window: i64, tab: i64) -> Result<(), String>;
    fn close_tab(&self, session: &str, window: i64, tab: i64) -> Result<(), String>;
    fn close_window(&self, session: &str, window: i64) -> Result<(), String>;
}

impl BrowserApi for Browser {
    fn snapshot(&self) -> Result<Snapshot, String> {
        self.snapshot()
    }
    fn create(&self, url: Option<&str>, window: bool) -> Result<Created, String> {
        self.create(url, window)
    }
    fn focus(&self, session: &str, window: i64, tab: i64) -> Result<(), String> {
        self.focus(session, window, tab)
    }
    fn close_tab(&self, session: &str, window: i64, tab: i64) -> Result<(), String> {
        self.close_tab(session, window, tab)
    }
    fn close_window(&self, session: &str, window: i64) -> Result<(), String> {
        self.close_window(session, window)
    }
}

pub fn run(
    command: Command,
    browser: &impl BrowserApi,
    state: &mut State,
) -> Result<String, String> {
    if command == Command::Help {
        return Ok(HELP.into());
    }
    let snapshot = browser.snapshot()?;
    let tabs = state.sync(&snapshot.session, &snapshot.tabs);
    match command {
        Command::New | Command::Open | Command::Search(_) => {
            let url = match &command {
                Command::Search(query) => Some(search_url(query)),
                _ => None,
            };
            let window = command == Command::Open;
            let created = browser.create(url.as_deref(), window)?;
            if state.session != created.session {
                state.sync(&created.session, &[]);
            }
            let key = TabKey {
                window_id: created.window_id,
                tab_id: created.tab_id,
            };
            state.opened_tabs.push(key);
            if !state.order.contains(&key) {
                state.order.push(key);
            }
            if window {
                state.opened_windows.push(created.window_id);
            }
            Ok(if window {
                "Opened window."
            } else {
                "Opened tab."
            }
            .into())
        }
        Command::Close => {
            let key = *state
                .opened_tabs
                .last()
                .ok_or("No open tabs created by hel.")?;
            browser.close_tab(&state.session, key.window_id, key.tab_id)?;
            state.opened_tabs.pop();
            state.order.retain(|tab| *tab != key);
            Ok("Closed tab.".into())
        }
        Command::Quit => {
            let window = *state
                .opened_windows
                .last()
                .ok_or("No open windows created by hel open.")?;
            browser.close_window(&state.session, window)?;
            state.opened_windows.pop();
            state.opened_tabs.retain(|tab| tab.window_id != window);
            state.order.retain(|tab| tab.window_id != window);
            Ok("Closed window.".into())
        }
        Command::List => {
            if tabs.is_empty() {
                return Ok("No open tabs.".into());
            }
            Ok(render(&tabs, &(0..tabs.len()).collect::<Vec<_>>()))
        }
        Command::Number(number) => {
            let tab = tabs.get(number - 1).ok_or("No match.")?;
            browser.focus(&state.session, tab.window_id, tab.tab_id)?;
            Ok(format!("Focused tab {number}."))
        }
        Command::Title(query) => {
            let matches = Index::new(&tabs).title(&query);
            if let [index] = matches.as_slice() {
                let tab = &tabs[*index];
                browser.focus(&state.session, tab.window_id, tab.tab_id)?;
                Ok(format!("Focused tab {}.", index + 1))
            } else {
                results(&tabs, &matches)
            }
        }
        Command::Site(site) => results(&tabs, &Index::new(&tabs).site(&site)),
        Command::SiteTitle(site, title) => {
            results(&tabs, &Index::new(&tabs).site_title(&site, &title))
        }
        Command::Help => unreachable!(),
    }
}

fn results(tabs: &[Tab], matches: &[usize]) -> Result<String, String> {
    if matches.is_empty() {
        Err("No match.".into())
    } else {
        Ok(render(tabs, matches))
    }
}

fn render(tabs: &[Tab], matches: &[usize]) -> String {
    matches
        .iter()
        .map(|index| {
            let tab = &tabs[*index];
            format!(
                "{}\twindow {}\t{}\t{}",
                index + 1,
                tab.window_id,
                printable(&tab.title),
                printable(&crate::search::hostname(&tab.url))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn printable(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn search_url(query: &str) -> String {
    let encoded: String = query
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("https://duckduckgo.com/?q={encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        events: RefCell<Vec<String>>,
        session: &'static str,
        fail: bool,
    }
    fn tabs() -> Vec<Tab> {
        [
            (1, "Rust book", "https://rust-lang.org"),
            (2, "Rust video", "https://youtube.com"),
            (3, "Music", "https://youtube.com"),
        ]
        .into_iter()
        .map(|(id, title, url)| Tab {
            window_id: id,
            tab_id: id + 10,
            title: title.into(),
            url: url.into(),
        })
        .collect()
    }
    impl Fake {
        fn new() -> Self {
            Self {
                events: RefCell::new(vec![]),
                session: "live",
                fail: false,
            }
        }
        fn event(&self, session: &str, event: String) -> Result<(), String> {
            assert_eq!(session, self.session);
            if self.fail {
                return Err("test failure".into());
            }
            self.events.borrow_mut().push(event);
            Ok(())
        }
    }
    impl BrowserApi for Fake {
        fn snapshot(&self) -> Result<Snapshot, String> {
            Ok(Snapshot {
                session: self.session.into(),
                tabs: tabs(),
            })
        }
        fn create(&self, url: Option<&str>, window: bool) -> Result<Created, String> {
            self.event(
                "live",
                format!("create {window} {}", url.unwrap_or("blank")),
            )?;
            Ok(Created {
                session: "live".into(),
                window_id: 4,
                tab_id: 14,
            })
        }
        fn focus(&self, s: &str, w: i64, t: i64) -> Result<(), String> {
            self.event(s, format!("focus {w} {t}"))
        }
        fn close_tab(&self, s: &str, w: i64, t: i64) -> Result<(), String> {
            self.event(s, format!("close {w} {t}"))
        }
        fn close_window(&self, s: &str, w: i64) -> Result<(), String> {
            self.event(s, format!("quit {w}"))
        }
    }

    #[test]
    fn search_text_is_url_encoded_and_created_tab_is_tracked() {
        let fake = Fake::new();
        let mut state = State::default();
        run(Command::Search("a & café\"".into()), &fake, &mut state).unwrap();
        assert_eq!(
            *fake.events.borrow(),
            ["create false https://duckduckgo.com/?q=a%20%26%20caf%C3%A9%22"]
        );
        assert_eq!(
            state.opened_tabs,
            [TabKey {
                window_id: 4,
                tab_id: 14
            }]
        );
        assert!(state.opened_windows.is_empty());
    }

    #[test]
    fn lists_filters_and_focuses_only_unambiguous_title_matches() {
        let fake = Fake::new();
        let mut state = State::default();
        assert!(
            run(Command::List, &fake, &mut state)
                .unwrap()
                .starts_with("1\t")
        );
        assert_eq!(
            run(Command::Site("youtube".into()), &fake, &mut state)
                .unwrap()
                .lines()
                .count(),
            2
        );
        assert_eq!(
            run(Command::Title("Rust".into()), &fake, &mut state)
                .unwrap()
                .lines()
                .count(),
            2
        );
        assert!(fake.events.borrow().is_empty());
        run(
            Command::SiteTitle("youtube".into(), "video".into()),
            &fake,
            &mut state,
        )
        .unwrap();
        assert!(fake.events.borrow().is_empty());
        run(Command::Title("book".into()), &fake, &mut state).unwrap();
        run(Command::Number(3), &fake, &mut state).unwrap();
        assert_eq!(*fake.events.borrow(), ["focus 1 11", "focus 3 13"]);
        for command in [
            Command::Number(99),
            Command::Title("missing".into()),
            Command::SiteTitle("missing".into(), "Rust".into()),
            Command::SiteTitle("youtube".into(), "missing".into()),
        ] {
            assert_eq!(run(command, &fake, &mut state).unwrap_err(), "No match.");
        }
    }

    #[test]
    fn closes_only_tracked_live_items_and_preserves_history_on_failure() {
        let mut fake = Fake::new();
        let mut state = State::default();
        assert!(run(Command::Close, &fake, &mut state).is_err());
        assert!(run(Command::Quit, &fake, &mut state).is_err());
        state.opened_tabs = vec![TabKey::from(&tabs()[0]), TabKey::from(&tabs()[2])];
        state.opened_windows = vec![1, 3];
        fake.fail = true;
        assert!(run(Command::Close, &fake, &mut state).is_err());
        assert_eq!(state.opened_tabs.len(), 2);
        fake.fail = false;
        run(Command::Close, &fake, &mut state).unwrap();
        run(Command::Quit, &fake, &mut state).unwrap();
        assert_eq!(*fake.events.borrow(), ["close 3 13", "quit 3"]);
        assert_eq!(state.opened_tabs, [TabKey::from(&tabs()[0])]);
        state.session = "old-session".into();
        assert!(run(Command::Quit, &fake, &mut state).is_err());
        assert!(state.opened_windows.is_empty());
    }

    #[test]
    fn blank_tabs_and_windows_are_tracked_and_help_is_available() {
        let fake = Fake::new();
        let mut state = State::default();
        run(Command::New, &fake, &mut state).unwrap();
        run(Command::Open, &fake, &mut state).unwrap();
        assert_eq!(state.opened_windows, [4]);
        assert!(
            run(Command::Help, &fake, &mut state)
                .unwrap()
                .contains("hel tab")
        );
        assert_eq!(
            printable("a\x1b[31m\nb\t日本語"),
            "a\\u{1b}[31m\\nb\\t日本語"
        );
    }
}
