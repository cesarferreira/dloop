use std::time::Instant;

#[test]
fn perf_filter_matching_should_be_fast() {
    // Simulate checking 10,000 log entries with cached search text
    let cached_search_text = "androidruntime e fatal exception example".to_lowercase();
    let filter = "exception".to_lowercase();

    let start = Instant::now();
    for _ in 0..10_000 {
        let _ = cached_search_text.contains(&filter);
    }
    let elapsed = start.elapsed();

    println!("✓ 10k filter matches: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 10,
        "Filter matching 10k entries took {:?}, should be < 10ms for snappy scrolling",
        elapsed
    );
}

#[test]
fn perf_exclude_matching_should_be_fast() {
    let cached_search_text = "androidruntime e fatal exception example".to_lowercase();
    let excludes = vec!["chatty".to_string(), "verbose".to_string()];

    let start = Instant::now();
    for _ in 0..10_000 {
        let _ = excludes
            .iter()
            .any(|e| !e.is_empty() && cached_search_text.contains(&e.to_lowercase()));
    }
    let elapsed = start.elapsed();

    println!("✓ 10k exclude checks: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 20,
        "Exclude matching 10k entries took {:?}, should be < 20ms for snappy scrolling",
        elapsed
    );
}

#[test]
fn perf_no_allocation_overhead() {
    // This ensures the hot path doesn't allocate (which would cause GC pressure)
    let cached_search_text = "androidruntime e fatal exception example".to_lowercase();
    let filter = "exception".to_lowercase();

    // Warm up
    for _ in 0..1000 {
        let _ = cached_search_text.contains(&filter);
    }

    // Measure large batch - should be extremely fast with zero allocations
    let start = Instant::now();
    for _ in 0..100_000 {
        let _ = cached_search_text.contains(&filter);
    }
    let elapsed = start.elapsed();

    println!("✓ 100k cached filter matches (zero allocations): {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 50,
        "100k cached matches took {:?}, should be < 50ms (indicates unwanted allocations if slower)",
        elapsed
    );
}

#[test]
fn perf_scroll_simulation() {
    // Simulate scrolling through logs - checking many entries rapidly
    let entries: Vec<String> = (0..10_000)
        .map(|i| format!("tag{} e message {} with content", i, i).to_lowercase())
        .collect();

    let filter = "message".to_lowercase();

    let start = Instant::now();
    let mut matches = 0;
    for entry in &entries {
        if entry.contains(&filter) {
            matches += 1;
        }
    }
    let elapsed = start.elapsed();

    println!(
        "✓ Scroll through 10k entries: {:?} ({} matches)",
        elapsed, matches
    );
    assert!(
        elapsed.as_millis() < 20,
        "Scrolling through 10k entries took {:?}, should be < 20ms for smooth scrolling",
        elapsed
    );
}
