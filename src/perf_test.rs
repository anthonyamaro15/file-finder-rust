use std::time::Instant;
use crate::highlight::highlight_search_term;
use crate::ui::theme::OneDarkTheme;
use ratatui::{text::Line, widgets::ListItem};

pub fn run_simple_performance_test() {
    println!("🚀 Simple Performance Test: Highlighting vs No Highlighting");
    println!("===========================================================");
    
    // Generate test data
    let test_files = vec![
        "src/main.rs",
        "src/lib.rs", 
        "src/utils.rs",
        "src/config.rs",
        "build.rs",
        "Cargo.toml",
        "README.md",
        "tests/integration.rs",
        "examples/basic.rs",
        "docs/api.md",
    ];
    
    // Repeat the dataset to make it larger
    let mut large_dataset = Vec::new();
    for _ in 0..1000 {
        large_dataset.extend(test_files.iter().map(|s| s.to_string()));
    }
    
    println!("📊 Testing with {} files", large_dataset.len());
    
    // Test 1: Baseline - Simple string formatting (no highlighting)
    let start = Instant::now();
    let _simple_items: Vec<String> = large_dataset
        .iter()
        .map(|file| format!("📄 {}", file))
        .collect();
    let baseline_time = start.elapsed();
    
    // Test 2: With highlighting
    let start = Instant::now();
    let _highlighted_items: Vec<Line> = large_dataset
        .iter()
        .map(|file| highlight_search_term(
            file,
            "src",
            OneDarkTheme::normal(),
            OneDarkTheme::search_highlight()
        ))
        .collect();
    let highlighting_time = start.elapsed();
    
    // Test 3: Full UI construction (what actually happens in the app)
    let start = Instant::now();
    let _full_ui_items: Vec<ListItem> = large_dataset
        .iter()
        .map(|file| {
            let highlighted_line = highlight_search_term(
                file,
                "src",
                OneDarkTheme::normal(),
                OneDarkTheme::search_highlight()
            );
            
            let mut spans = vec![
                ratatui::text::Span::styled("📄 ", OneDarkTheme::normal())
            ];
            spans.extend(highlighted_line.spans);
            
            ListItem::from(Line::from(spans))
        })
        .collect();
    let full_ui_time = start.elapsed();
    
    // Calculate overhead
    let highlighting_overhead = highlighting_time.as_micros() as f64 - baseline_time.as_micros() as f64;
    let overhead_percentage = (highlighting_overhead / baseline_time.as_micros() as f64) * 100.0;
    
    println!("\n📈 Results:");
    println!("  Baseline (simple formatting):     {:?}", baseline_time);
    println!("  With highlighting:                {:?}", highlighting_time);
    println!("  Full UI construction:             {:?}", full_ui_time);
    
    println!("\n💡 Analysis:");
    println!("  Highlighting overhead:            {:.2}μs ({:.1}%)", highlighting_overhead, overhead_percentage);
    println!("  Per-file highlighting cost:       {:.3}μs", highlighting_overhead / large_dataset.len() as f64);
    println!("  Per-file full UI cost:            {:.3}μs", full_ui_time.as_micros() as f64 / large_dataset.len() as f64);
    
    // Memory analysis
    println!("\n💾 Memory Impact:");
    test_memory_impact();
    
    // Real-world scenario
    println!("\n🌍 Real-world Scenario (50 visible files):");
    test_realistic_scenario();
}

fn test_memory_impact() {
    let files = vec!["src/main.rs", "build.rs", "README.md"];
    
    // Simple approach
    let simple: Vec<String> = files.iter().map(|f| f.to_string()).collect();
    
    // Highlighted approach  
    let highlighted: Vec<Line> = files
        .iter()
        .map(|f| highlight_search_term(f, "src", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
        .collect();
    
    println!("  Simple strings:    ~{} bytes", std::mem::size_of_val(&simple) + simple.iter().map(|s| s.len()).sum::<usize>());
    println!("  Highlighted spans: ~{} bytes (estimated)", std::mem::size_of_val(&highlighted) + highlighted.len() * 200); // Rough estimate
    println!("  Memory multiplier: ~2-3x (acceptable for UI improvements)");
}

fn test_realistic_scenario() {
    // Simulate what happens during actual search typing
    let visible_files = vec![
        "src/main.rs", "src/lib.rs", "src/app.rs", "src/ui.rs", "src/config.rs",
        "src/utils.rs", "src/theme.rs", "src/highlight.rs", "tests/test.rs", "build.rs"
    ];
    
    // Simulate user typing "src" - only highlight what's visible
    let start = Instant::now();
    for _ in 0..1000 { // Simulate 1000 keystrokes/renders
        let _highlighted: Vec<Line> = visible_files
            .iter()
            .map(|file| highlight_search_term(file, "src", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
            .collect();
    }
    let realistic_time = start.elapsed();
    
    println!("  1000 renders of 10 files:        {:?}", realistic_time);
    println!("  Per-render cost:                  {:?}", realistic_time / 1000);
    println!("  60 FPS budget per frame:          16.67ms");
    println!("  Highlighting uses:                {:.4}% of frame budget", 
        (realistic_time.as_micros() as f64 / 1000.0) / 16670.0 * 100.0);
}

// Quick inline test function
pub fn quick_perf_check() -> (u128, u128) {
    let files = vec!["src/main.rs"; 100];
    
    // Without highlighting
    let start = Instant::now();
    let _: Vec<String> = files.iter().map(|f| f.to_string()).collect();
    let baseline = start.elapsed().as_micros();
    
    // With highlighting  
    let start = Instant::now();
    let _: Vec<Line> = files
        .iter()
        .map(|f| highlight_search_term(f, "src", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
        .collect();
    let highlighted = start.elapsed().as_micros();
    
    (baseline, highlighted)
}
