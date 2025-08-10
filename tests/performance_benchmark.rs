/// Performance benchmark test to demonstrate connection pooling improvements
use p2p_play::storage::*;
use std::time::Instant;
use tempfile::TempDir;
use tokio::task::JoinSet;
use uuid::Uuid;

#[tokio::test]
async fn benchmark_database_operations() {
    println!("🚀 Running database performance benchmark...");
    
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("benchmark_{}.db", Uuid::new_v4().simple());
    let db_path = temp_dir.path().join(&db_name);
    unsafe {
        std::env::set_var("TEST_DATABASE_PATH", db_path.to_str().unwrap());
    }

    // Reset connection and initialize storage
    reset_db_connection_for_testing()
        .await
        .expect("Failed to reset connection");
    ensure_stories_file_exists()
        .await
        .expect("Failed to initialize storage");

    // Benchmark: Sequential operations
    let start_sequential = Instant::now();
    for i in 0..20 {
        create_new_story_with_channel(
            &format!("Sequential Story {}", i),
            "Header",
            "Body",
            "benchmark"
        ).await.expect("Failed to create story");
    }
    let sequential_duration = start_sequential.elapsed();

    // Reset for concurrent test
    reset_db_connection_for_testing()
        .await
        .expect("Failed to reset connection");
    ensure_stories_file_exists()
        .await
        .expect("Failed to initialize storage");

    // Benchmark: Concurrent operations with connection pooling
    let start_concurrent = Instant::now();
    let mut join_set = JoinSet::new();
    
    for i in 0..20 {
        join_set.spawn(async move {
            create_new_story_with_channel(
                &format!("Concurrent Story {}", i),
                "Header", 
                "Body",
                "benchmark"
            ).await
        });
    }

    let mut concurrent_successes = 0;
    while let Some(result) = join_set.join_next().await {
        if let Ok(Ok(_)) = result {
            concurrent_successes += 1;
        }
    }
    let concurrent_duration = start_concurrent.elapsed();

    // Display results
    println!("📊 Benchmark Results:");
    println!("   Sequential: {:?} for 20 operations", sequential_duration);
    println!("   Concurrent: {:?} for 20 operations ({} succeeded)", 
             concurrent_duration, concurrent_successes);

    if concurrent_duration < sequential_duration {
        let improvement = ((sequential_duration.as_millis() as f64 - concurrent_duration.as_millis() as f64) 
                          / sequential_duration.as_millis() as f64) * 100.0;
        println!("   🎯 Performance improvement: {:.1}% faster with connection pooling!", improvement);
    }

    // Test pool stats
    if let Some((connections, idle, max_size)) = get_pool_stats().await {
        println!("   📈 Pool stats: {}/{} connections active (max: {})", 
                 connections, max_size, max_size);
    }

    // Verify data integrity
    let stories = read_local_stories().await.expect("Failed to read stories");
    let benchmark_stories = stories.iter()
        .filter(|s| s.channel == "benchmark")
        .count();
    
    println!("   ✅ Data integrity: {} benchmark stories created", benchmark_stories);
    assert!(benchmark_stories > 0, "Should have created benchmark stories");
    
    println!("✅ Performance benchmark completed successfully!");
}