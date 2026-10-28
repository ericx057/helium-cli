use std::cell::OnceCell;
use std::collections::BTreeMap;

const INDEX_DEPTH: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tab {
    pub window_id: i64,
    pub tab_id: i64,
    pub title: String,
    pub url: String,
}

#[derive(Default)]
struct Node {
    children: BTreeMap<char, usize>,
    tabs: Vec<usize>,
}

pub struct Index {
    nodes: OnceCell<Vec<Node>>,
    titles: Vec<String>,
    hosts: Vec<String>,
}

impl Index {
    pub fn new(tabs: &[Tab]) -> Self {
        Self {
            nodes: OnceCell::new(),
            titles: tabs.iter().map(|tab| tab.title.to_lowercase()).collect(),
            hosts: tabs.iter().map(|tab| hostname(&tab.url)).collect(),
        }
    }

    fn build_nodes(&self) -> Vec<Node> {
        let mut nodes = vec![Node::default()];
        for (tab, entry) in self.titles.iter().enumerate() {
            let title: Vec<char> = entry.chars().collect();
            // shortcut: index eight characters per suffix and verify full matches; use a compressed suffix tree if candidate filtering becomes a bottleneck.
            for start in 0..title.len() {
                let mut node = 0;
                for character in title[start..].iter().take(INDEX_DEPTH) {
                    let next = match nodes[node].children.get(character) {
                        Some(next) => *next,
                        None => {
                            let next = nodes.len();
                            nodes.push(Node::default());
                            nodes[node].children.insert(*character, next);
                            next
                        }
                    };
                    if nodes[next].tabs.last() != Some(&tab) {
                        nodes[next].tabs.push(tab);
                    }
                    node = next;
                }
            }
        }
        nodes
    }

    pub fn title(&self, query: &str) -> Vec<usize> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let query = query.to_lowercase();
        let nodes = self.nodes.get_or_init(|| self.build_nodes());
        let mut node = 0;
        for character in query.chars().take(INDEX_DEPTH) {
            match nodes[node].children.get(&character) {
                Some(next) => node = *next,
                None => return Vec::new(),
            }
        }
        nodes[node]
            .tabs
            .iter()
            .copied()
            .filter(|index| self.titles[*index].contains(&query))
            .collect()
    }

    pub fn site(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        let query = query.strip_prefix("www.").unwrap_or(&query);
        if query.is_empty() {
            return Vec::new();
        }
        self.hosts
            .iter()
            .enumerate()
            .filter_map(|(index, host)| host.contains(query).then_some(index))
            .collect()
    }

    pub fn site_title(&self, site: &str, title: &str) -> Vec<usize> {
        let sites = self.site(site);
        if sites.is_empty() {
            return Vec::new();
        }
        self.title(title)
            .into_iter()
            .filter(|index| sites.binary_search(index).is_ok())
            .collect()
    }
}

pub fn hostname(url: &str) -> String {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return String::new();
    };
    if !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return String::new();
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or("");
    let address = authority.rsplit('@').next().unwrap_or("");
    let (host, port) = if let Some(address) = address.strip_prefix('[') {
        let Some((host, tail)) = address.split_once(']') else {
            return String::new();
        };
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return String::new();
        }
        (host, tail)
    } else {
        let host = address.split(':').next().unwrap_or("");
        (host, &address[host.len()..])
    };
    if !port.is_empty()
        && !port
            .strip_prefix(':')
            .is_some_and(|port| !port.is_empty() && port.parse::<u16>().is_ok())
    {
        return String::new();
    }
    if host.is_empty()
        || host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "\\[]".contains(c))
    {
        return String::new();
    }
    let host = host.to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<Tab> {
        [
            (
                "Rust tutorial - YouTube",
                "https://www.youtube.com/watch?v=1",
            ),
            ("RUST book", "https://doc.rust-lang.org/book/"),
            ("Banana", "https://example.org/youtube?q=youtube.com"),
            ("CAFÉ 東京", "https://music.youtube.com:443/watch?v=2"),
            (
                "Rust tutorial - YouTube",
                "https://user:password@example.com/",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (title, url))| Tab {
            window_id: 1,
            tab_id: index as i64,
            title: title.into(),
            url: url.into(),
        })
        .collect()
    }

    #[test]
    fn title_matches_any_character_position_in_original_order() {
        let index = Index::new(&tabs());
        assert_eq!(index.title("UST"), vec![0, 1, 4]);
        assert_eq!(index.title("tutorial - you"), vec![0, 4]);
        assert_eq!(index.title("ana"), vec![2]);
        assert_eq!(index.title("a"), vec![0, 2, 3, 4]);
    }

    #[test]
    fn title_handles_unicode_and_missing_or_blank_queries() {
        let index = Index::new(&tabs());
        assert_eq!(index.title("café"), vec![3]);
        assert_eq!(index.title("京"), vec![3]);
        assert_eq!(index.title("unknown"), Vec::<usize>::new());
        assert_eq!(index.title(""), Vec::<usize>::new());
        assert_eq!(index.title("  "), Vec::<usize>::new());
        assert_eq!(Index::new(&[]).title("rust"), Vec::<usize>::new());
    }

    #[test]
    fn site_matches_only_hostname() {
        let index = Index::new(&tabs());
        assert!(index.nodes.get().is_none());
        assert_eq!(index.site("YOUTUBE"), vec![0, 3]);
        assert_eq!(index.site("www.youtube.com"), vec![0, 3]);
        assert_eq!(index.site("example"), vec![2, 4]);
        assert_eq!(index.site("password"), Vec::<usize>::new());
        assert_eq!(index.site("unknown"), Vec::<usize>::new());
        assert_eq!(index.site(""), Vec::<usize>::new());
        assert_eq!(index.site("  "), Vec::<usize>::new());
        assert!(index.nodes.get().is_none());
    }

    #[test]
    fn long_queries_verify_the_whole_title_after_the_indexed_prefix() {
        let tabs = ["abcdefgh first", "abcdefgh second", "abcdefgh first again"]
            .into_iter()
            .enumerate()
            .map(|(index, title)| Tab {
                window_id: 1,
                tab_id: index as i64,
                title: title.into(),
                url: "https://example.com/".into(),
            })
            .collect::<Vec<_>>();
        let index = Index::new(&tabs);
        assert_eq!(index.title("abcdefgh"), vec![0, 1, 2]);
        assert_eq!(index.title("abcdefgh FIRST"), vec![0, 2]);
        assert_eq!(index.title("abcdefgh missing"), Vec::<usize>::new());
    }

    #[test]
    fn long_titles_have_a_linear_bound_on_index_nodes() {
        let title = (0x1000..0x1800)
            .filter_map(char::from_u32)
            .collect::<String>();
        let query = title.chars().skip(100).take(40).collect::<String>();
        let length = title.chars().count();
        let index = Index::new(&[Tab {
            window_id: 1,
            tab_id: 1,
            title,
            url: "https://example.com/".into(),
        }]);
        assert_eq!(index.title(&query), vec![0]);
        assert!(index.nodes.get().unwrap().len() <= 1 + 8 * length);
    }

    #[test]
    fn combined_search_requires_both_site_and_title() {
        let index = Index::new(&tabs());
        assert_eq!(index.site_title("youtube", "tutorial"), vec![0]);
        assert_eq!(index.site_title("youtube", "café"), vec![3]);
        assert_eq!(index.site_title("missing", "rust"), Vec::<usize>::new());
        assert_eq!(index.site_title("youtube", "missing"), Vec::<usize>::new());
        assert_eq!(index.site_title("", "rust"), Vec::<usize>::new());
        assert_eq!(index.site_title("youtube", ""), Vec::<usize>::new());
    }

    #[test]
    fn hostname_handles_authority_boundaries_ports_and_ipv6() {
        assert_eq!(hostname("HTTPS://WWW.Example.COM:443/a?b#c"), "example.com");
        assert_eq!(hostname("https://youtube.com@example.org/a"), "example.org");
        assert_eq!(hostname("https://example.org?youtube.com"), "example.org");
        assert_eq!(hostname("https://example.org#youtube.com"), "example.org");
        assert_eq!(hostname("http://[::1]:8080/a"), "::1");
        assert_eq!(hostname("custom://localhost:123/"), "localhost");
        assert_eq!(hostname("https://BÜCHER.example/"), "bücher.example");
    }

    #[test]
    fn hostname_rejects_non_authority_and_malformed_urls() {
        for url in [
            "about:blank",
            "file:///tmp/youtube",
            "data:text/plain,youtube",
            "youtube.com/path",
            "https:///youtube",
            "https://bad host/",
            "https://[::1",
            "https://example.com:bad/",
            "1bad://example.com",
            "https://[::1]bad/",
            "https://example.com:65536/",
        ] {
            assert_eq!(hostname(url), "", "{url}");
        }
    }
}
