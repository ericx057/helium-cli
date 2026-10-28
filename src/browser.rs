use crate::search::Tab;
use serde::{Deserialize, de::DeserializeOwned};
use std::process::{Command, Output};

pub struct Browser;

pub struct Snapshot {
    pub session: String,
    pub tabs: Vec<Tab>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Created {
    pub session: String,
    pub window_id: i64,
    pub tab_id: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshot {
    session: String,
    tabs: Vec<WireTab>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTab {
    window_id: i64,
    tab_id: i64,
    title: String,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
struct AutomationError {
    error: String,
}

fn parse_snapshot(wire: WireSnapshot) -> Result<Snapshot, String> {
    if (!wire.tabs.is_empty() && wire.session.is_empty())
        || wire
            .tabs
            .iter()
            .any(|tab| tab.window_id <= 0 || tab.tab_id <= 0)
    {
        return Err("Helium returned invalid tab identifiers".to_owned());
    }
    Ok(Snapshot {
        session: wire.session,
        tabs: wire
            .tabs
            .into_iter()
            .map(|tab| Tab {
                window_id: tab.window_id,
                tab_id: tab.tab_id,
                title: tab.title,
                url: tab.url,
            })
            .collect(),
    })
}

fn decode_output<T: DeserializeOwned>(output: Output) -> Result<T, String> {
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(if detail.contains("-1743") {
            "Allow your terminal to control Helium in System Settings > Privacy & Security > Automation"
                .to_owned()
        } else {
            "Helium automation failed; check that Helium is installed and Automation access is allowed"
                .to_owned()
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|_| {
        serde_json::from_slice::<AutomationError>(&output.stdout)
            .map(|error| error.error)
            .unwrap_or_else(|_| "Helium returned invalid automation data".to_owned())
    })
}

impl Browser {
    fn call<T: DeserializeOwned>(&self, action: &str, arguments: &[&str]) -> Result<T, String> {
        if !cfg!(target_os = "macos") {
            return Err("hel currently supports macOS only".to_owned());
        }
        let output = Command::new("/usr/bin/osascript")
            .args([
                "-l",
                "JavaScript",
                "-e",
                include_str!("helium.js"),
                "--",
                action,
            ])
            .args(arguments)
            .output()
            .map_err(|_| "Could not start macOS automation".to_owned())?;
        decode_output(output)
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        parse_snapshot(self.call("snapshot", &[])?)
    }

    pub fn create(&self, url: Option<&str>, window: bool) -> Result<Created, String> {
        let created: Created = self.call(
            "create",
            &[
                if window { "window" } else { "tab" },
                url.unwrap_or("chrome://newtab/"),
            ],
        )?;
        if created.session.is_empty() || created.window_id <= 0 || created.tab_id <= 0 {
            return Err("Helium returned invalid creation identifiers".to_owned());
        }
        Ok(created)
    }

    pub fn focus(&self, session: &str, window_id: i64, tab_id: i64) -> Result<(), String> {
        self.call::<Empty>(
            "focus",
            &[session, &window_id.to_string(), &tab_id.to_string()],
        )?;
        Ok(())
    }

    pub fn close_tab(&self, session: &str, window_id: i64, tab_id: i64) -> Result<(), String> {
        self.call::<Empty>(
            "close-tab",
            &[session, &window_id.to_string(), &tab_id.to_string()],
        )?;
        Ok(())
    }

    pub fn close_window(&self, session: &str, window_id: i64) -> Result<(), String> {
        self.call::<Empty>("close-window", &[session, &window_id.to_string()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(value: serde_json::Value) -> Result<Snapshot, String> {
        let mut output = Command::new("/usr/bin/true").output().unwrap();
        output.stdout = serde_json::to_vec(&value).unwrap();
        parse_snapshot(decode_output(output)?)
    }

    #[test]
    fn snapshot_preserves_special_characters() {
        let snapshot = snapshot(json!({
            "session": "123:456", "tabs": [{
                "window_id": 12, "tab_id": 34,
                "title": "A\tB\nC 日本語 \"quoted\"", "url": "https://example.com/"
            }]
        }))
        .unwrap();
        assert_eq!(snapshot.session, "123:456");
        assert_eq!(snapshot.tabs[0].title, "A\tB\nC 日本語 \"quoted\"");
    }

    #[test]
    fn stopped_browser_has_empty_snapshot() {
        let snapshot = snapshot(json!({"session": "", "tabs": []})).unwrap();
        assert!(snapshot.tabs.is_empty());
    }

    #[test]
    fn rejects_malformed_snapshot_and_missing_session() {
        for value in [
            json!({}),
            json!({"session":"a", "tabs":[{"window_id":"oops"}]}),
            json!({"session":"", "tabs":[{"window_id":1,"tab_id":2,"title":"x","url":"x"}]}),
            json!({"session":"a", "tabs":[{"window_id":-1,"tab_id":2,"title":"x","url":"x"}]}),
        ] {
            assert!(snapshot(value).is_err());
        }
    }

    #[test]
    fn script_failures_do_not_expose_output() {
        let mut output = Command::new("/usr/bin/false").output().unwrap();
        output.stderr = b"private search text".to_vec();
        let error = decode_output::<Empty>(output).unwrap_err();
        assert!(!error.contains("private"));
    }

    #[test]
    fn permission_failure_is_actionable() {
        let mut output = Command::new("/usr/bin/false").output().unwrap();
        output.stderr = b"Not authorized (-1743)".to_vec();
        assert!(
            decode_output::<Empty>(output)
                .unwrap_err()
                .contains("Privacy & Security")
        );
    }

    #[test]
    fn invalid_json_does_not_expose_browser_content() {
        let mut output = Command::new("/usr/bin/true").output().unwrap();
        output.stdout = b"private title".to_vec();
        assert_eq!(
            decode_output::<Empty>(output).unwrap_err(),
            "Helium returned invalid automation data"
        );
    }

    #[test]
    fn mutation_error_envelope_is_not_mistaken_for_success() {
        let mut output = Command::new("/usr/bin/true").output().unwrap();
        output.stdout = br#"{"error":"That tab is no longer open"}"#.to_vec();
        assert_eq!(
            decode_output::<Empty>(output).unwrap_err(),
            "That tab is no longer open"
        );
    }

    #[test]
    fn escaped_automation_errors_are_preserved() {
        let mut output = Command::new("/usr/bin/true").output().unwrap();
        output.stdout = br#"{"error":"Could not select \"tab\""}"#.to_vec();
        assert_eq!(
            decode_output::<Empty>(output).unwrap_err(),
            "Could not select \"tab\""
        );
    }
}
