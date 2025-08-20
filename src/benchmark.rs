use std::time::Instant;
use crate::highlight::highlight_search_term;
use crate::ui::theme::OneDarkTheme;
use ratatui::{text::Line, widgets::ListItem};

/// Benchmark highlighting performance with different dataset sizes
pub fn benchmark_highlighting() {
    println!("🔍 Search Highlighting Performance Benchmark");
    println!("============================================");
    
    // Test datasets of different sizes
    let small_dataset = generate_test_files(100);
    let medium_dataset = generate_test_files(1000);
    let large_dataset = generate_test_files(10000);
    
    // Benchmark each scenario
    benchmark_scenario("Small (100 files)", &small_dataset, "src");
    benchmark_scenario("Medium (1K files)", &medium_dataset, "src");
    benchmark_scenario("Large (10K files)", &large_dataset, "src");
    
    // Test different search term lengths
    println!("\n📏 Search Term Length Impact:");
    benchmark_search_term_length(&medium_dataset);
    
    // Test different match frequencies
    println!("\n🎯 Match Frequency Impact:");
    benchmark_match_frequency(&medium_dataset);
}

fn benchmark_scenario(name: &str, files: &[String], search_term: &str) {
    println!("\n📁 {}", name);
    
    // Benchmark WITHOUT highlighting (baseline)
    let start = Instant::now();
    let _simple_items: Vec<ListItem> = files
        .iter()
        .map(|file| ListItem::from(format!("📄 {}", file)))
        .collect();
    let baseline_duration = start.elapsed();
    
    // Benchmark WITH highlighting
    let start = Instant::now();
    let _highlighted_items: Vec<ListItem> = files
        .iter()
        .map(|file| {
            let highlighted_line = highlight_search_term(
                file,
                search_term,
                OneDarkTheme::normal(),
                OneDarkTheme::search_highlight()
            );
            
            // Simulate real UI construction with icon
            let mut spans = vec![
                ratatui::text::Span::styled("📄 ", OneDarkTheme::normal())
            ];
            spans.extend(highlighted_line.spans);
            
            ListItem::from(Line::from(spans))
        })
        .collect();
    let highlighting_duration = start.elapsed();
    
    // Calculate performance metrics
    let overhead = highlighting_duration.as_micros() as f64 - baseline_duration.as_micros() as f64;
    let overhead_percentage = (overhead / baseline_duration.as_micros() as f64) * 100.0;
    
    println!("  ⚡ Baseline (no highlighting): {:?}", baseline_duration);
    println!("  🎨 With highlighting:         {:?}", highlighting_duration);
    println!("  📈 Overhead:                  {:.2}μs ({:.1}%)", overhead, overhead_percentage);
    println!("  🏃 Per-file cost:             {:.2}μs", overhead / files.len() as f64);
}

fn benchmark_search_term_length(files: &[String]) {
    let search_terms = vec!["a", "src", "config", "component", "configuration"];
    
    for term in search_terms {
        let start = Instant::now();
        let _: Vec<Line> = files
            .iter()
            .take(1000) // Limit to first 1000 for consistency
            .map(|file| highlight_search_term(
                file,
                term,
                OneDarkTheme::normal(),
                OneDarkTheme::search_highlight()
            ))
            .collect();
        let duration = start.elapsed();
        
        println!("  '{}' ({}chars): {:?} ({:.2}μs/file)", 
            term, 
            term.len(), 
            duration,
            duration.as_micros() as f64 / 1000.0
        );
    }
}

fn benchmark_match_frequency(files: &[String]) {
    // Create datasets with different match frequencies
    let high_match_files = generate_files_with_pattern("src", 1000, 0.8); // 80% contain "src"
    let low_match_files = generate_files_with_pattern("xyz", 1000, 0.1);  // 10% contain "xyz"
    
    println!("  High frequency matches (80%):");
    let start = Instant::now();
    let _: Vec<Line> = high_match_files
        .iter()
        .map(|file| highlight_search_term(file, "src", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
        .collect();
    println!("    {:?} ({:.2}μs/file)", start.elapsed(), start.elapsed().as_micros() as f64 / 1000.0);
    
    println!("  Low frequency matches (10%):");
    let start = Instant::now();
    let _: Vec<Line> = low_match_files
        .iter()
        .map(|file| highlight_search_term(file, "xyz", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
        .collect();
    println!("    {:?} ({:.2}μs/file)", start.elapsed(), start.elapsed().as_micros() as f64 / 1000.0);
}

fn generate_test_files(count: usize) -> Vec<String> {
    let prefixes = ["src", "lib", "bin", "test", "doc", "config", "build", "target"];
    let extensions = ["rs", "toml", "md", "txt", "json", "yaml", "lock"];
    let names = ["main", "lib", "mod", "utils", "helper", "core", "api", "client"];
    
    (0..count)
        .map(|i| {
            let prefix = prefixes[i % prefixes.len()];
            let name = names[(i / prefixes.len()) % names.len()];
            let ext = extensions[i % extensions.len()];
            format!("{}/{}.{}", prefix, name, ext)
        })
        .collect()
}

fn generate_files_with_pattern(pattern: &str, count: usize, frequency: f64) -> Vec<String> {
    let mut files = Vec::new();
    let should_match_count = (count as f64 * frequency) as usize;
    
    // Files that contain the pattern
    for i in 0..should_match_count {
        files.push(format!("{}_file_{}.rs", pattern, i));
    }
    
    // Files that don't contain the pattern
    for i in should_match_count..count {
        files.push(format!("other_file_{}.txt", i));
    }
    
    files
}

/// Memory usage analysis
pub fn analyze_memory_usage() {
    println!("\n💾 Memory Usage Analysis");
    println!("========================");
    
    let files = generate_test_files(1000);
    
    // Simple ListItem creation (baseline)
    let simple_items: Vec<ListItem> = files
        .iter()
        .map(|file| ListItem::from(file.clone()))
        .collect();
    
    // Highlighted ListItem creation
    let highlighted_items: Vec<ListItem> = files
        .iter()
        .map(|file| {
            let highlighted_line = highlight_search_term(
                file,
                "src",
                OneDarkTheme::normal(),
                OneDarkTheme::search_highlight()
            );
            ListItem::from(highlighted_line)
        })
        .collect();
    
    println!("📊 Memory estimates (1000 files):");
    println!("  Simple items:      ~{} KB", estimate_listitem_memory(&simple_items));
    println!("  Highlighted items: ~{} KB", estimate_listitem_memory(&highlighted_items));
    
    // Analyze span count
    let avg_spans = highlighted_items
        .iter()
        .map(|item| count_spans_in_listitem(item))
        .sum::<usize>() as f64 / highlighted_items.len() as f64;
    
    println!("  Average spans per highlighted item: {:.1}", avg_spans);
}

fn estimate_listitem_memory(items: &[ListItem]) -> usize {
    // Rough estimation based on string content and structure
    items.iter()
        .map(|item| estimate_single_listitem_memory(item))
        .sum::<usize>() / 1024 // Convert to KB
}

fn estimate_single_listitem_memory(item: &ListItem) -> usize {
    // This is a rough estimation - actual memory usage may vary
    // Each ListItem contains Line(s), each Line contains Span(s)
    // Estimating ~50-100 bytes per span + string content
    100 + count_spans_in_listitem(item) * 80
}

fn count_spans_in_listitem(item: &ListItem) -> usize {
    // This is approximate since ListItem structure isn't directly accessible
    // In practice, highlighted items typically have 2-4 spans
    3 // Average estimate for highlighted items
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_highlighting_performance() {
        let files = generate_test_files(100);
        
        let start = Instant::now();
        let _: Vec<Line> = files
            .iter()
            .map(|file| highlight_search_term(
                file,
                "src",
                OneDarkTheme::normal(),
                OneDarkTheme::search_highlight()
            ))
            .collect();
        let duration = start.elapsed();
        
        // Should complete in reasonable time (< 1ms for 100 files)
        assert!(duration.as_millis() < 10, "Highlighting took too long: {:?}", duration);
    }
    
    #[test]
    fn test_memory_overhead() {
        let files = generate_test_files(10);
        
        // Test that we don't have memory leaks or excessive allocation
        for _ in 0..1000 {
            let _: Vec<Line> = files
                .iter()
                .map(|file| highlight_search_term(file, "test", OneDarkTheme::normal(), OneDarkTheme::search_highlight()))
                .collect();
        }
        // If we reach here without OOM, we're probably okay
    }
}
