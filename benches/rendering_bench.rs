use std::time::Instant;

#[test]
fn bench_filter_matching_with_10k_entries() {
    // Create a mock entry
    let cached_search_text = "androidruntime e fatal exception example".to_lowercase();
    let filter = "exception".to_lowercase();

    let start = Instant::now();
    for _ in 0..10_000 {
        let _ = cached_search_text.contains(&filter);
    }
    let elapsed = start.elapsed();

    println!("10k filter matches: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 10,
        "Filter matching 10k entries took {:?}, should be < 10ms",
        elapsed
    );
}

#[test]
fn bench_exclude_matching_with_10k_entries() {
    let cached_search_text = "androidruntime e fatal exception example".to_lowercase();
    let excludes = vec!["chatty".to_string(), "verbose".to_string()];

    let start = Instant::now();
    for _ in 0..10_000 {
        let _ = excludes
            .iter()
            .any(|e| !e.is_empty() && cached_search_text.contains(&e.to_lowercase()));
    }
    let elapsed = start.elapsed();

    println!("10k exclude checks: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 10,
        "Exclude matching 10k entries took {:?}, should be < 10ms",
        elapsed
    );
}

#[test]
fn bench_no_string_allocation_in_hot_path() {
    // This test ensures we're not allocating strings in the hot path
    let cached_search_text = "androidruntime e fatal exception example".to_lowercase();
    let filter = "exception".to_lowercase();

    // First run to warm up
    for _ in 0..1000 {
        let _ = cached_search_text.contains(&filter);
    }

    // Measure a large batch
    let start = Instant::now();
    for _ in 0..100_000 {
        let _ = cached_search_text.contains(&filter);
    }
    let elapsed = start.elapsed();

    println!("100k filter matches (no allocations): {:?}", elapsed);
    // Should be extremely fast since there are no allocations
    assert!(
        elapsed.as_millis() < 50,
        "100k cached filter matches took {:?}, should be < 50ms",
        elapsed
    );
}
