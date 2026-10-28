use std::borrow::Cow;
use std::cell::OnceCell;

const INDEX_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tab {
    pub window_id: i64,
    pub tab_id: i64,
    pub title: String,
    pub url: String,
}

struct LetterTree {
    alphabet: Vec<u64>,
    masks: Vec<u64>,
    ascii: [u16; 128],
    words: usize,
    depth: usize,
    bloom: [u64; 32],
    ascii_titles: bool,
}

pub struct Index<'a> {
    tabs: &'a [Tab],
    tree: OnceCell<Option<LetterTree>>,
}

impl<'a> Index<'a> {
    pub fn new(tabs: &'a [Tab]) -> Self {
        Self {
            tabs,
            tree: OnceCell::new(),
        }
    }

    fn build_tree(&self) -> Option<LetterTree> {
        let depth = if self.tabs.iter().all(|tab| tab.title.is_ascii()) {
            3
        } else {
            1
        };
        let mut bloom = [0u64; 32];
        if depth > 1 {
            for tab in self.tabs {
                for token in tokens(&tab.title, 3, true) {
                    let bit = bloom_bit(token);
                    bloom[bit / 64] |= 1 << (bit % 64);
                }
            }
        }
        (1..=depth)
            .rev()
            .find_map(|size| self.build_depth(size, bloom, depth > 1))
    }

    fn build_depth(
        &self,
        depth: usize,
        bloom: [u64; 32],
        ascii_titles: bool,
    ) -> Option<LetterTree> {
        let words = self.tabs.len().div_ceil(64);
        let capacity = (INDEX_BYTES - std::mem::size_of::<Self>())
            .checked_div(words.checked_mul(8)?.checked_add(8)?)?
            .min(u16::MAX as usize);
        if capacity == 0 || words == 0 {
            return None;
        }
        let mut alphabet = Vec::with_capacity(capacity);
        for tab in self.tabs {
            for character in tokens(&tab.title, depth, true) {
                if let Err(position) = alphabet.binary_search(&character) {
                    if alphabet.len() == capacity {
                        return None;
                    }
                    alphabet.insert(position, character);
                }
            }
        }
        let mut tree = LetterTree {
            masks: vec![0; alphabet.len() * words],
            alphabet,
            ascii: [u16::MAX; 128],
            words,
            depth,
            bloom,
            ascii_titles,
        };
        for (row, character) in tree.alphabet.iter().enumerate() {
            if *character > 0 && *character <= 128 {
                tree.ascii[*character as usize - 1] = row as u16;
            }
        }
        for (tab, entry) in self.tabs.iter().enumerate() {
            for character in tokens(&entry.title, depth, true) {
                let row = tree.row(character)?;
                tree.masks[row * words + tab / 64] |= 1 << (tab % 64);
            }
        }
        Some(tree)
    }

    pub fn title(&self, query: &str) -> Vec<usize> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let query = lowercase(query);
        let tree = self.tree.get_or_init(|| self.build_tree());
        let Some(tree) = tree else {
            return self
                .tabs
                .iter()
                .enumerate()
                .filter_map(|(i, tab)| contains(&tab.title, &query).then_some(i))
                .collect();
        };
        if tree.ascii_titles {
            for token in tokens(&query, 3, false) {
                let bit = bloom_bit(token);
                if tree.bloom[bit / 64] & (1 << (bit % 64)) == 0 {
                    return Vec::new();
                }
            }
        }
        let mut result = Vec::new();
        for word in 0..tree.words {
            let mut candidates = u64::MAX;
            for character in tokens(&query, tree.depth, false) {
                let Some(row) = tree.row(character) else {
                    return Vec::new();
                };
                candidates &= tree.masks[row * tree.words + word];
                if candidates == 0 {
                    break;
                }
            }
            while candidates != 0 {
                let bit = candidates.trailing_zeros() as usize;
                let index = word * 64 + bit;
                if contains(&self.tabs[index].title, &query) {
                    result.push(index);
                }
                candidates &= candidates - 1;
            }
        }
        result
    }

    pub fn site(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        let query = query.strip_prefix("www.").unwrap_or(&query);
        if query.is_empty() {
            return Vec::new();
        }
        self.tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| {
                host_slice(&tab.url)
                    .filter(|host| contains(host, query))
                    .map(|_| index)
            })
            .collect()
    }

    pub fn site_title(&self, site: &str, title: &str) -> Vec<usize> {
        let sites = self.site(site);
        if sites.is_empty() {
            return Vec::new();
        }
        if title.trim().is_empty() {
            return Vec::new();
        }
        let title = lowercase(title);
        sites
            .into_iter()
            .filter(|index| contains(&self.tabs[*index].title, &title))
            .collect()
    }
}

impl LetterTree {
    fn row(&self, character: u64) -> Option<usize> {
        if character > 0 && character <= 128 {
            let row = self.ascii[character as usize - 1];
            (row != u16::MAX).then_some(row as usize)
        } else {
            self.alphabet.binary_search(&character).ok()
        }
    }
}

fn lowercase(value: &str) -> Cow<'_, str> {
    if value.is_ascii() && !value.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(value.to_lowercase())
    }
}

fn contains(value: &str, lowercase_query: &str) -> bool {
    if value.is_ascii() {
        let needle = lowercase_query.as_bytes();
        let Some(first) = needle.first() else {
            return false;
        };
        if !first.is_ascii() {
            return false;
        }
        memchr::memchr2_iter(*first, first.to_ascii_uppercase(), value.as_bytes()).any(|start| {
            value
                .as_bytes()
                .get(start..start + needle.len())
                .is_some_and(|window| window.eq_ignore_ascii_case(needle))
        })
    } else {
        value.to_lowercase().contains(lowercase_query)
    }
}

fn letters(value: &str) -> impl Iterator<Item = char> + '_ {
    // Uppercase sigma can lowercase contextually to either Greek sigma form.
    value
        .chars()
        .flat_map(char::to_lowercase)
        .chain(value.contains('Σ').then_some('ς'))
}

fn tokens(value: &str, depth: usize, prefixes: bool) -> impl Iterator<Item = u64> + '_ {
    let mut rolling = 0u64;
    let mut count = 0;
    letters(value).flat_map(move |character| {
        rolling = ((rolling << 21) | (u64::from(character) + 1)) & (u64::MAX >> 1);
        count += 1;
        let length = count.min(depth);
        (1..=length).filter_map(move |size| {
            let selected = if prefixes {
                size == 1 || size == depth
            } else if length < depth {
                size == 1
            } else {
                size == depth
            };
            selected.then_some(rolling & ((1u64 << (21 * size)) - 1))
        })
    })
}

fn bloom_bit(token: u64) -> usize {
    (token.wrapping_mul(0x9e3779b97f4a7c15) >> 53) as usize
}

pub fn hostname(url: &str) -> String {
    host_slice(url).unwrap_or("").to_lowercase()
}

fn host_slice(url: &str) -> Option<&str> {
    let (scheme, remainder) = url.split_once("://")?;
    if !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return None;
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or("");
    let address = authority.rsplit('@').next().unwrap_or("");
    let (host, port) = if let Some(address) = address.strip_prefix('[') {
        let (host, tail) = address.split_once(']')?;
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return None;
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
        return None;
    }
    if host.is_empty()
        || host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "\\[]".contains(c))
    {
        return None;
    }
    Some(
        if host
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www."))
        {
            &host[4..]
        } else {
            host
        },
    )
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
        let tabs = tabs();
        let index = Index::new(&tabs);
        assert_eq!(index.title("UST"), vec![0, 1, 4]);
        assert_eq!(index.title("tutorial - you"), vec![0, 4]);
        assert_eq!(index.title("ana"), vec![2]);
        assert_eq!(index.title("a"), vec![0, 2, 3, 4]);
    }

    #[test]
    fn title_handles_unicode_and_missing_or_blank_queries() {
        let tabs = tabs();
        let index = Index::new(&tabs);
        assert_eq!(index.title("café"), vec![3]);
        assert_eq!(index.title("京"), vec![3]);
        assert_eq!(index.title("unknown"), Vec::<usize>::new());
        assert_eq!(index.title(""), Vec::<usize>::new());
        assert_eq!(index.title("  "), Vec::<usize>::new());
        assert_eq!(Index::new(&[]).title("rust"), Vec::<usize>::new());
    }

    #[test]
    fn site_matches_only_hostname() {
        let tabs = tabs();
        let index = Index::new(&tabs);
        assert!(index.tree.get().is_none());
        assert_eq!(index.site("YOUTUBE"), vec![0, 3]);
        assert_eq!(index.site("www.youtube.com"), vec![0, 3]);
        assert_eq!(index.site("example"), vec![2, 4]);
        assert_eq!(index.site("password"), Vec::<usize>::new());
        assert_eq!(index.site("unknown"), Vec::<usize>::new());
        assert_eq!(index.site(""), Vec::<usize>::new());
        assert_eq!(index.site("  "), Vec::<usize>::new());
        assert!(index.tree.get().is_none());
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
        let tabs = [Tab {
            window_id: 1,
            tab_id: 1,
            title,
            url: "https://example.com/".into(),
        }];
        let index = Index::new(&tabs);
        assert_eq!(index.title(&query), vec![0]);
        assert!(length > 1000);
        assert!(index.tree.get().unwrap().is_none());
    }

    #[test]
    fn combined_search_requires_both_site_and_title() {
        let tabs = tabs();
        let index = Index::new(&tabs);
        assert_eq!(index.site_title("youtube", "tutorial"), vec![0]);
        assert_eq!(index.site_title("youtube", "café"), vec![3]);
        assert_eq!(index.site_title("missing", "rust"), Vec::<usize>::new());
        assert_eq!(index.site_title("youtube", "missing"), Vec::<usize>::new());
        assert_eq!(index.site_title("", "rust"), Vec::<usize>::new());
        assert_eq!(index.site_title("youtube", ""), Vec::<usize>::new());
    }

    #[test]
    fn character_tree_is_bounded_and_verifies_character_order() {
        let tabs = ["ab", "ba", "aab", "ΟΣ", "ΟΣΑ", "İstanbul"]
            .into_iter()
            .enumerate()
            .map(|(id, title)| Tab {
                window_id: 1,
                tab_id: id as i64,
                title: title.into(),
                url: String::new(),
            })
            .collect::<Vec<_>>();
        let index = Index::new(&tabs);
        assert_eq!(index.title("ab"), vec![0, 2]);
        assert_eq!(index.title("aab"), vec![2]);
        assert_eq!(index.title("ος"), vec![3]);
        assert_eq!(index.title("οσ"), vec![4]);
        assert_eq!(index.title("İST"), vec![5]);
        let tree = index.tree.get().unwrap().as_ref().unwrap();
        assert!(
            std::mem::size_of::<LetterTree>()
                + tree.alphabet.capacity() * 8
                + tree.masks.capacity() * 8
                <= INDEX_BYTES
        );
    }

    #[test]
    fn bitsets_work_across_word_boundaries_and_match_a_reference_scan() {
        let tabs = (0..193)
            .map(|id| Tab {
                window_id: 1,
                tab_id: id,
                title: format!("Project {id} Rust CAFÉ"),
                url: String::new(),
            })
            .collect::<Vec<_>>();
        let index = Index::new(&tabs);
        for query in [
            "Project 64",
            "Project 192",
            "Rust",
            "Café",
            "missing",
            "reordered",
            "project 6",
        ] {
            let expected = tabs
                .iter()
                .enumerate()
                .filter_map(|(id, tab)| {
                    tab.title
                        .to_lowercase()
                        .contains(&query.to_lowercase())
                        .then_some(id)
                })
                .collect::<Vec<_>>();
            assert_eq!(index.title(query), expected);
        }
    }

    #[test]
    fn short_substrings_and_permutations_match_reference_at_each_depth() {
        let tabs = (0..100)
            .map(|id| Tab {
                window_id: 1,
                tab_id: id,
                title: format!(
                    "Project {id}: Rust browser automation notes and tutorial | Example"
                ),
                url: String::new(),
            })
            .collect::<Vec<_>>();
        let index = Index::new(&tabs);
        for length in 1..=12 {
            for start in 0..30 {
                let query = &tabs[12].title[start..start + length];
                let expected = tabs
                    .iter()
                    .enumerate()
                    .filter_map(|(id, tab)| {
                        (!query.trim().is_empty()
                            && tab.title.to_lowercase().contains(&query.to_lowercase()))
                        .then_some(id)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(index.title(query), expected, "{query:?}");
            }
        }
        let tree = index.tree.get().unwrap().as_ref().unwrap();
        assert!(
            std::mem::size_of::<Index>() + tree.alphabet.capacity() * 8 + tree.masks.capacity() * 8
                <= INDEX_BYTES
        );
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
