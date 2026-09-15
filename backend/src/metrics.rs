//! In-process Prometheus metrics (iter-loop-20260901 r5).
//!
//! Why hand-rolled and not `metrics-exporter-prometheus`:
//!   * The crate pulls ~30 transitive deps, which clashes with the project's
//!     "no heavy deps" constraint.
//!   * The metric set we need is tiny (3 families, all counters), so the
//!     whole renderer is < 200 lines and avoids the static-init
//!     `metrics::set_global_recorder` dance.
//!
//! Trade-offs the reader should know about:
//!   * This is counters-only. No histograms / gauges / summaries. If we ever
//!     need latency histograms we should swap to the real crate rather than
//!     extend this one — bucketing is fiddly and easy to get wrong.
//!   * Labels are unconstrained `String` pairs. We must NOT pass unbounded
//!     values (e.g. raw user IDs as a label) — that would explode the
//!     cardinality. The HTTP middleware falls back to the matched route
//!     pattern (`/decisions/:id/status`), never the raw path.
//!   * The registry is process-global. Tests use `reset_for_tests()` to
//!     isolate assertions; production code never touches it.

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// One label vector -> one monotonic counter. Labels are stored sorted
/// (BTreeMap) so the Prometheus render order is stable and tests can
/// substring-match against the output deterministically.
type LabelMap = BTreeMap<String, String>;

#[derive(Default)]
struct CounterSeries {
    /// Map from sorted-label-tuple -> atomic counter. BTreeMap gives stable
    /// iteration for the text-format render without hashing concerns.
    series: BTreeMap<Vec<(String, String)>, AtomicU64>,
}

static COUNTERS: Mutex<BTreeMap<String, CounterSeries>> = Mutex::new(BTreeMap::new());

/// Increment a counter by 1. `labels` may be empty. Label keys are sorted on
/// insert; values are passed through verbatim — the caller is responsible for
/// not sending unbounded values (see module docs).
pub fn counter_inc(name: &str, labels: LabelMap) {
    counter_add(name, labels, 1);
}

/// Increment a counter by `delta`. Same constraints as `counter_inc`.
pub fn counter_add(name: &str, labels: LabelMap, delta: u64) {
    let mut guard = COUNTERS.lock().expect("metrics registry poisoned");
    let series = guard.entry(name.to_string()).or_default();
    let mut key: Vec<(String, String)> = labels.into_iter().collect();
    key.sort();
    series
        .series
        .entry(key)
        .or_insert(AtomicU64::new(0))
        .fetch_add(delta, Ordering::Relaxed);
}

/// Snapshot of every counter, used by the renderer. Returning a Vec (rather
/// than holding the lock during text formatting) keeps the lock window tiny
/// and avoids blocking HTTP handlers on I/O.
#[derive(Debug)]
pub struct CounterSnapshot {
    pub name: String,
    pub series: Vec<(LabelMap, u64)>,
}

pub fn snapshot() -> Vec<CounterSnapshot> {
    let guard = COUNTERS.lock().expect("metrics registry poisoned");
    guard
        .iter()
        .map(|(name, cs)| CounterSnapshot {
            name: name.clone(),
            series: cs
                .series
                .iter()
                .map(|(k, v)| {
                    let labels: LabelMap = k.iter().cloned().collect();
                    (labels, v.load(Ordering::Relaxed))
                })
                .collect(),
        })
        .collect()
}

/// Reset every counter to 0. Test-only: there is no legitimate use in
/// production, and exposing it under a non-cfg(test) gate would let a buggy
/// dashboard silently zero a critical metric.
#[cfg(test)]
pub fn reset_for_tests() {
    let mut guard = COUNTERS.lock().expect("metrics registry poisoned");
    guard.clear();
}

/// Render the registry to Prometheus text format (v0.0.4).
///
/// Format reference: <https://prometheus.io/docs/instrumenting/exposition_formats/>
///
/// Layout per metric family:
///   # HELP <name> <description>
///   # TYPE <name> counter
///   <name>{k="v",k2="v2"} <value>
///   <name> <value>     # when no labels
///
/// Label values are escaped for backslash, double-quote, and newline. The
/// help text is escaped for backslash and newline only (no quotes).
///
/// We always emit the family header for every known metric (see KNOWN_FAMILIES)
/// even when there are no samples yet. Prometheus scrapes that arrive before
/// the first event would otherwise silently drop the series; emitting a zero
/// value up-front makes the family discoverable.
pub fn render_prometheus(snap: &[CounterSnapshot]) -> String {
    let mut out = String::with_capacity(256);
    for (name, help) in KNOWN_FAMILIES {
        out.push_str(&format!("# HELP {} {}\n", name, escape_help(help)));
        out.push_str(&format!("# TYPE {} counter\n", name));

        let matching: Vec<&(LabelMap, u64)> = snap
            .iter()
            .find(|c| c.name == *name)
            .map(|c| c.series.iter().collect())
            .unwrap_or_default();

        if matching.is_empty() {
            out.push_str(&format!("{} 0\n", name));
            continue;
        }

        for (labels, value) in matching {
            if labels.is_empty() {
                out.push_str(&format!("{} {}\n", name, value));
            } else {
                let rendered = labels
                    .iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, escape_label_value(v)))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("{}{{{}}} {}\n", name, rendered, value));
            }
        }
    }
    out
}

/// Known metric families. The renderer emits a header + zero-value line for
/// every family in this list, regardless of whether any samples have been
/// recorded yet — that lets Prometheus discover the families on its first
/// scrape (e.g. right after a deploy).
///
/// Adding a new metric:
///   1. Pick a stable name (Prometheus convention: `<noun>_total` for counters).
///   2. Add an entry here with a short HELP line.
///   3. Start calling `counter_inc` / `counter_add` from the relevant handler.
///   4. Add a render test that pins the HELP/TYPE/zero-value output.
const KNOWN_FAMILIES: &[(&str, &str)] = &[
    (
        "http_requests_total",
        "Total HTTP requests handled, labeled by service/method/path/status",
    ),
    (
        "decisions_total",
        "Decision-shaped events recorded by the handler layer, labeled by action",
    ),
    (
        "privacy_share_total",
        "Decision share-scope transitions, labeled by resulting scope",
    ),
];

fn escape_label_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

fn escape_help(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// `axum` middleware that records `http_requests_total{service,method,path,status}`
/// for every request that reaches it. Mount this on the WHOLE router (above
/// the protected/unprotected split) so we capture both API calls AND the
/// unauthenticated `/metrics` scrape itself — observability of the
/// observability endpoint is a feature, not a bug.
///
/// `path` is the matched route pattern (`/api/v1/decisions/:id/status`) when
/// available, falling back to a literal `unmatched` for 404s. We never use
/// the raw URI as the label — that would let an attacker explode cardinality
/// with one curl per fake path.
pub async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    // MatchedPath is set by the router AFTER pattern matching. If the request
    // didn't match (404), this is None and we label it `unmatched` rather
    // than the raw URI — see the cardinality note above.
    let path: String = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());

    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();

    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    labels.insert("service".to_string(), "jiabu-server".to_string());
    labels.insert("method".to_string(), method.as_str().to_string());
    labels.insert("path".to_string(), path);
    labels.insert("status".to_string(), status);
    counter_inc("http_requests_total", labels);

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(pairs: &[(&str, &str)]) -> LabelMap {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn counter_inc_creates_series_at_zero_then_increments() {
        reset_for_tests();
        counter_inc("test_metric_a", label(&[]));
        counter_inc("test_metric_a", label(&[]));
        counter_inc("test_metric_a", label(&[]));
        let snap = snapshot();
        let m = snap.iter().find(|c| c.name == "test_metric_a").unwrap();
        assert_eq!(m.series.len(), 1);
        assert_eq!(m.series[0].0, LabelMap::new());
        assert_eq!(m.series[0].1, 3);
    }

    #[test]
    fn labels_partition_into_separate_series() {
        reset_for_tests();
        counter_inc("test_metric_b", label(&[("method", "GET")]));
        counter_inc("test_metric_b", label(&[("method", "GET")]));
        counter_inc("test_metric_b", label(&[("method", "POST")]));
        let snap = snapshot();
        let m = snap.iter().find(|c| c.name == "test_metric_b").unwrap();
        assert_eq!(m.series.len(), 2);
        let get_total = m
            .series
            .iter()
            .find(|(l, _)| l.get("method").map(|v| v.as_str()) == Some("GET"))
            .unwrap()
            .1;
        let post_total = m
            .series
            .iter()
            .find(|(l, _)| l.get("method").map(|v| v.as_str()) == Some("POST"))
            .unwrap()
            .1;
        assert_eq!(get_total, 2);
        assert_eq!(post_total, 1);
    }

    #[test]
    fn render_includes_help_and_type_headers() {
        reset_for_tests();
        counter_inc("http_requests_total", label(&[("path", "/health")]));
        let text = render_prometheus(&snapshot());
        assert!(text.contains("# HELP http_requests_total "), "{}", text);
        assert!(text.contains("# TYPE http_requests_total counter"), "{}", text);
        assert!(
            text.contains("http_requests_total{path=\"/health\"} 1"),
            "{}",
            text
        );
    }

    #[test]
    fn render_emits_zero_when_no_samples() {
        // Discovery matters: Prometheus needs the family header even before
        // any traffic arrives, otherwise first-scrape-after-deploy drops the
        // series silently.
        reset_for_tests();
        let text = render_prometheus(&snapshot());
        assert!(text.contains("# TYPE decisions_total counter"));
        assert!(text.contains("decisions_total 0\n"));
    }

    #[test]
    fn render_handles_label_keys_in_sorted_order() {
        // Order of label keys must be deterministic for test substring matches
        // and to avoid spurious diffs in dashboards.
        reset_for_tests();
        counter_inc(
            "http_requests_total",
            label(&[("status", "200"), ("method", "GET"), ("path", "/x")]),
        );
        let text = render_prometheus(&snapshot());
        let line = text
            .lines()
            .find(|l| l.starts_with("http_requests_total{"))
            .unwrap();
        // sorted alphabetically: method, path, status
        let m = line.find("method=").unwrap();
        let p = line.find("path=").unwrap();
        let s = line.find("status=").unwrap();
        assert!(m < p && p < s, "expected method<path<status, got {}", line);
    }

    #[test]
    fn render_escapes_special_chars_in_label_values() {
        reset_for_tests();
        counter_inc(
            "http_requests_total",
            label(&[("path", "a\"b\\c\nd")]),
        );
        let text = render_prometheus(&snapshot());
        assert!(text.contains("path=\"a\\\"b\\\\c\\nd\""), "{}", text);
    }

    #[test]
    fn counter_add_with_explicit_delta() {
        reset_for_tests();
        counter_add("decisions_total", label(&[("action", "created")]), 7);
        counter_add("decisions_total", label(&[("action", "created")]), 3);
        let snap = snapshot();
        let m = snap.iter().find(|c| c.name == "decisions_total").unwrap();
        assert_eq!(m.series[0].1, 10);
    }

    #[test]
    fn reset_clears_all_series() {
        counter_inc("decisions_total", label(&[("action", "x")]));
        counter_inc("http_requests_total", label(&[("path", "/y")]));
        reset_for_tests();
        assert!(snapshot().is_empty());
    }
}
