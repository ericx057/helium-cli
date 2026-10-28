# Search performance

The persistent search index has an 8,192-byte budget, including the index object and requested heap storage. When its keys do not fit, it uses fewer sequence levels or a scan. It does not truncate titles or results. This meets the requested 10 KB index-storage bound. It does not bound input data, returned results, allocator overhead, process memory, or temporary Unicode normalization.

The requested **5 ns p95 title lookup has not been achieved**. Query normalization, sequence lookup, substring verification, and returning variable numbers of matches all contribute. No cached-result shortcut or precompiled-query benchmark is presented as an ordinary search.

## Reproduce

```sh
cargo bench --bench search --locked
```

The standalone benchmark uses synthetic 20, 100, and 1,000-tab datasets. Titles follow `Project N: Rust browser automation notes and tutorial | Example`. It reports one build timing, persistent index bytes, build peak bytes, warmed batches of 100 lookups, and 5,000 individually timed lookups per query. Allocator accounting is disabled during latency sampling. Input fixtures and result vectors are excluded from persistent index bytes; the build peak includes transient query/results. The reported individual p95 includes clock overhead and quantization. Batch-mean p95 is a separate statistic and does not establish individual-call p95.

These are local observations on macOS 26.6.2 / aarch64 with Rust 1.99.1, not a latency guarantee. CPU scheduling and clock quantization produce variation. The initial commit `0598708` was remeasured in a temporary checkout with the same benchmark source and settings.

| Dataset / metric | Initial version | Compact index |
| --- | ---: | ---: |
| 100 tabs: persistent index | 1,081,538 B | 7,896 B |
| 1,000 tabs: persistent index | 8,237,706 B | 5,008 B |
| 100 tabs: initial build + first lookup | 540,500 ns | 217,833 ns |
| 1,000 tabs: initial build + first lookup | 3,513,083 ns | 1,259,250 ns |
| 100 tabs: selective `Project 12:` individual p95 | 917 ns | 417 ns |
| 100 tabs: absent `nonexistent` individual p95 | 42 ns | 83 ns |
| 100 tabs: broad `rust` individual p95 | 667 ns | 1,125 ns |
| 1,000 tabs: selective individual p95 | 4,625 ns | 2,375 ns |
| 1,000 tabs: absent individual p95 | 42 ns | 42 ns |
| 1,000 tabs: broad individual p95 | 5,291 ns | 9,375 ns |

The memory reduction is over 99% on these larger fixtures. Selective lookup and construction improve; broad-result lookup is slower because the bounded index verifies borrowed, case-preserving titles instead of retaining lowercase copies. The earlier character-only variant was rejected because absent queries could trigger expensive scans. Short sequences and a global Bloom filter improve that case without unbounded storage. Very large alphabets use the exact scan fallback and can be slower.

## Command execution

The shell launcher resolves npm symlinks and replaces itself with Rust using `exec`; Node runs during installation only. A warmed 20-sample `--help` comparison measured the previous Node launcher at 23.87 ms p95 and about 40.52 MB peak RSS, versus 7.12 ms p95 and about 1.59 MB through a shell-launcher symlink. Those process measurements are separate from search lookup/index measurements.

Opening a tab, window, or web search now performs one automation call and does not snapshot unrelated tabs first. Browser responses deserialize directly into Rust types. Unicode and escaping tests, a bounded-index test, reference substring comparisons, and the live browser smoke test guard correctness.
