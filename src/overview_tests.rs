use super::*;

#[test]
fn an_empty_summary_reports_zeros_without_targets() {
    assert_eq!(
        render(&Summary::default()),
        "queries: 0 · 0 complete · 0 partial · 0 failed\n\
         tokens: 0 in / 0 out · reported by 0 queries\n\
         median complete query: - wall · - to first token\n\
         history: 0 threads · 0 turns · 0 threads cleared by expiry\n"
    );
}

#[test]
fn targets_report_historical_health() {
    let summary = Summary {
        queries: 3,
        complete: 1,
        partial: 1,
        failed: 1,
        input_tokens: 12,
        output_tokens: 3,
        with_usage: 1,
        median_wall_ms: Some(1_250),
        median_first_token_ms: Some(99),
        threads: 1,
        turns: 1,
        threads_cleared: 1,
        targets: vec![
            TargetHealth {
                kind: "openai-compatible".to_string(),
                base_url: "http://127.0.0.1:1/v1".to_string(),
                model: "one".to_string(),
                queries: 1,
                last_success_at_ms: Some(0),
                last_success_source: Some("query".to_string()),
                last_failure_at_ms: Some(60_000),
                last_failure_class: Some("timeout".to_string()),
                last_failure_source: Some("live-check".to_string()),
            },
            TargetHealth {
                kind: "openai-compatible".to_string(),
                base_url: "http://127.0.0.1:1/v1".to_string(),
                model: "two".to_string(),
                queries: 2,
                last_success_at_ms: None,
                last_success_source: None,
                last_failure_at_ms: None,
                last_failure_class: None,
                last_failure_source: None,
            },
        ],
    };
    assert_eq!(
        render(&summary),
        "queries: 3 · 1 complete · 1 partial · 1 failed\n\
         tokens: 12 in / 3 out · reported by 1 query\n\
         median complete query: 1.2s wall · 0.0s to first token\n\
         history: 1 thread · 1 turn · 1 thread cleared by expiry\n\
         \n\
         provider targets (historical observations, not a current check):\n\
         openai-compatible · http://127.0.0.1:1/v1 · one\n  \
         1 query · last observed healthy 1970-01-01 00:00 UTC (from query) · last failure 1970-01-01 00:01 UTC (timeout) (from live check)\n\
         openai-compatible · http://127.0.0.1:1/v1 · two\n  \
         2 queries · never observed healthy · no failures observed\n"
    );
}
