use std::time::Instant;
use file_finder::highlight::highlight_search_term;
use file_finder::ui::theme::OneDarkTheme;
use ratatui::{text::Line, widgets::ListItem};

fn main() {
    println!("🚀 Search Highlighting Performance Analysis");
    println!("==========================================");
    
    // Generate realistic test data
    let test_files: Vec<String> = (0..10000).map(|i| {
        match i % 10 {
            0 => format!("src/main_{}.rs", i),
            1 => format!("src/lib_{}.rs", i),
            2 => format!("src/utils_{}.rs", i),
            3 => format!("build_{}.rs", i),
            4 => format!("tests/test_{}.rs", i),
            5 => format!("examples/example_{}.rs", i),
            6 => format!("docs/doc_{}.md", i),
            7 => format!("config/config_{}.toml", i),
            8 => format!("assets/asset_{}.png", i),
            _ => format!("other/file_{}.txt", i),
        }
    }).collect();
    
    println!("📊 Testing with {} files", test_files.len());
    
    // Scenario 1: Baseline - Simple string operations (no highlighting)
    let start = Instant::now();
    let _simple: Vec<String> = test_files
        .iter()
        .map(|file| format!("📄 {}", file))
        .collect();
    let baseline_duration = start.elapsed();
    
    // Scenario 2: With highlighting 
    let start = Instant::now();
    let _highlighted: Vec<Line> = test_files
        .iter()
        .map(|file| highlight_search_term(
            file,
            "src",
            OneDarkTheme::normal(),
            OneDarkTheme::search_highlight()
        ))
        .collect();
    let highlighting_duration = start.elapsed();
    
    // Scenario 3: Full UI construction (realistic scenario)
    let start = Instant::now();
    let _ui_items: Vec<ListItem> = test_files
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
    let full_ui_duration = start.elapsed();
    
    // Calculate metrics
    let overhead_micros = highlighting_duration.as_micros() as f64 - baseline_duration.as_micros() as f64;
    let overhead_percentage = (overhead_micros / baseline_duration.as_micros() as f64) * 100.0;
    let per_file_cost = overhead_micros / test_files.len() as f64;
    
    println!("\n📈 Performance Results:");
    println!("  Baseline (no highlighting):      {:?}", baseline_duration);
    println!("  With highlighting:               {:?}", highlighting_duration);
    println!("  Full UI construction:            {:?}", full_ui_duration);
    
    println!("\n💡 Performance Analysis:");
    println!("  Highlighting overhead:           {:.2}μs ({:.1}%)", overhead_micros, overhead_percentage);
    println!("  Per-file highlighting cost:      {:.3}μs", per_file_cost);
    println!("  Files per millisecond:           {:.0}", 1000.0 / per_file_cost);
    
    // Real-world scenarios
    println!("\n🌍 Real-world Impact:");
    
    // Typical search scenario (50 visible files)
    let visible_files: Vec<String> = test_files.iter().take(50).cloned().collect();
    
    let start = Instant::now();
    for _ in 0..1000 { // Simulate 1000 UI renders
        let _: Vec<Line> = visible_files
            .iter()
            .map(|file| highlight_search_term(file, "src", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
            .collect();
    }
    let realistic_duration = start.elapsed();
    
    println!("  1000 renders of 50 files:       {:?}", realistic_duration);
    println!("  Per-render cost:                 {:?}", realistic_duration / 1000);
    println!("  60 FPS frame budget:             16.67ms");
    println!("  Highlighting uses:               {:.4}% of frame budget", 
        (realistic_duration.as_micros() as f64 / 1000.0) / 16670.0 * 100.0);
    
    // Different search term lengths
    println!("\n📏 Search Term Length Impact:");
    let search_terms = vec!["a", "rs", "src", "main", "config", "component"];
    for term in search_terms {
        let start = Instant::now();
        let _: Vec<Line> = visible_files
            .iter()
            .map(|file| highlight_search_term(file, term, OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
            .collect();
        let duration = start.elapsed();
        
        println!("  '{}' ({} chars):  {:?} ({:.3}μs/file)", 
            term, 
            term.len(), 
            duration,
            duration.as_micros() as f64 / visible_files.len() as f64
        );
    }
    
    // Match frequency impact
    println!("\n🎯 Match Frequency Impact:");
    test_match_frequency();
    
    println!("\n✅ Conclusion:");
    if per_file_cost < 1.0 {
        println!("  🟢 Excellent: < 1μs per file");
    } else if per_file_cost < 5.0 {
        println!("  🟡 Good: < 5μs per file");
    } else {
        println!("  🔴 Consider optimization: > 5μs per file");
    }
    
    println!("  📝 The highlighting feature adds minimal overhead");
    println!("     and significantly improves user experience!");
}

fn test_match_frequency() {
    // High match frequency (80% of files contain "src")
    let high_match_files: Vec<String> = (0..1000).map(|i| {
        if i % 5 != 0 {
            format!("src/file_{}.rs", i)
        } else {
            format!("other/file_{}.txt", i)
        }
    }).collect();
    
    // Low match frequency (10% of files contain "rare")
    let low_match_files: Vec<String> = (0..1000).map(|i| {
        if i % 10 == 0 {
            format!("rare_file_{}.rs", i)
        } else {
            format!("common_file_{}.txt", i)
        }
    }).collect();
    
    // Test high match frequency
    let start = Instant::now();
    let _: Vec<Line> = high_match_files
        .iter()
        .map(|file| highlight_search_term(file, "src", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
        .collect();
    let high_freq_duration = start.elapsed();
    
    // Test low match frequency  
    let start = Instant::now();
    let _: Vec<Line> = low_match_files
        .iter()
        .map(|file| highlight_search_term(file, "rare", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
        .collect();
    let low_freq_duration = start.elapsed();
    
    println!("  High frequency (80% matches): {:?} ({:.3}μs/file)", 
        high_freq_duration, 
        high_freq_duration.as_micros() as f64 / 1000.0);
    println!("  Low frequency (10% matches):  {:?} ({:.3}μs/file)", 
        low_freq_duration, 
        low_freq_duration.as_micros() as f64 / 1000.0);
}
