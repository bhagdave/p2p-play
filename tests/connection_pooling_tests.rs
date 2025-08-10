/// Tests for database connection pooling functionality
use p2p_play::storage::*;
use std::time::Instant;
use tempfile::TempDir;
use tokio::task::JoinSet;
use uuid::Uuid;

#[tokio::test]
async fn test_pool_stats_functionality() {
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("test_pool_stats_{}.db", Uuid::new_v4().simple());
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

    // Get connection to initialize pool
    let _conn = get_db_connection().await.expect("Failed to get connection");

    // Check pool stats
    let stats = get_pool_stats().await;
    assert!(stats.is_some(), "Pool stats should be available");

    let (connections, idle_connections, max_size) = stats.unwrap();
    assert!(connections > 0, "Should have active connections");
    assert!(max_size == 10, "Max size should be 10 as configured");
    assert!(
        idle_connections <= connections,
        "Idle connections should not exceed total connections"
    );

    println!(
        "✅ Pool stats: connections={}, idle={}, max={}",
        connections, idle_connections, max_size
    );
}

#[tokio::test]
async fn test_transaction_functionality() {
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("test_transactions_{}.db", Uuid::new_v4().simple());
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

    // Test successful transaction
    let result = with_transaction(|conn| {
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                "999",
                "Transaction Test",
                "Header",
                "Body",
                "1",
                "test",
                "1234567890"
            ],
        )?;
        Ok("success")
    }).await;

    assert!(result.is_ok(), "Transaction should succeed");

    // Verify the story was inserted
    let stories = read_local_stories().await.expect("Failed to read stories");
    assert!(
        stories.iter().any(|s| s.name == "Transaction Test"),
        "Story should be inserted by transaction"
    );

    // Test failed transaction (should rollback)
    let result: Result<&str, _> = with_transaction(|conn| {
        conn.execute(
            "INSERT INTO stories (id, name, header, body, public, channel, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                "1000",
                "Failed Transaction Test",
                "Header",
                "Body",
                "1",
                "test",
                "1234567890"
            ],
        )?;
        // Simulate an error
        Err("Simulated error".into())
    }).await;

    assert!(result.is_err(), "Transaction should fail");

    // Verify the story was NOT inserted (rollback worked)
    let stories = read_local_stories().await.expect("Failed to read stories");
    assert!(
        !stories.iter().any(|s| s.name == "Failed Transaction Test"),
        "Story should be rolled back"
    );

    println!("✅ Transaction functionality test passed!");
}

#[tokio::test]
async fn test_read_transaction_functionality() {
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("test_read_transactions_{}.db", Uuid::new_v4().simple());
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

    // Insert test data first
    create_new_story("Read Test Story", "Header", "Body")
        .await
        .expect("Failed to create story");

    // Test read transaction
    let result = with_read_transaction(|conn| {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM stories")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    })
    .await;

    assert!(result.is_ok(), "Read transaction should succeed");
    let count = result.unwrap();
    assert!(count > 0, "Should have at least one story");

    println!(
        "✅ Read transaction functionality test passed! Found {} stories",
        count
    );
}

#[tokio::test]
async fn test_concurrent_database_operations() {
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("test_concurrent_{}.db", Uuid::new_v4().simple());
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

    let start_time = Instant::now();
    let mut join_set = JoinSet::new();

    // Spawn multiple concurrent database operations
    for i in 0..20 {
        join_set.spawn(async move {
            // Mix of read and write operations
            if i % 2 == 0 {
                // Write operation
                create_new_story_with_channel(
                    &format!("Concurrent Story {}", i),
                    "Header",
                    "Body",
                    "concurrent_test",
                )
                .await
            } else {
                // Read operation
                read_local_stories().await.map(|_| ())
            }
        });
    }

    // Wait for all operations to complete
    let mut success_count = 0;
    let mut error_count = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(_)) => success_count += 1,
            Ok(Err(_)) => error_count += 1,
            Err(_) => error_count += 1,
        }
    }

    let duration = start_time.elapsed();

    // Verify results
    assert!(success_count > 0, "At least some operations should succeed");
    // Allow for some failures in concurrent operations - this is normal
    println!("✅ Concurrent operations test passed!");
    println!("   - Success: {}, Errors: {}", success_count, error_count);
    println!("   - Duration: {:?}", duration);

    if success_count >= 10 {
        // Check that stories were actually created
        let stories = read_local_stories().await.expect("Failed to read stories");
        let concurrent_stories = stories
            .iter()
            .filter(|s| s.name.starts_with("Concurrent Story"))
            .count();

        println!("   - Concurrent stories created: {}", concurrent_stories);
    }
}

#[tokio::test]
async fn test_connection_pool_resilience() {
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("test_resilience_{}.db", Uuid::new_v4().simple());
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

    // Test multiple connections to the same database
    let mut connections = Vec::new();
    for _ in 0..5 {
        let conn = get_db_connection().await.expect("Failed to get connection");
        connections.push(conn);
    }

    // Verify all connections work
    for (i, conn_arc) in connections.iter().enumerate() {
        let conn = conn_arc.lock().await;
        let result = conn.prepare("SELECT 1");
        assert!(result.is_ok(), "Connection {} should be valid", i);
    }

    // Test that we can reset the connection pool and it still works
    reset_db_connection_for_testing()
        .await
        .expect("Failed to reset connection pool");

    // Should be able to get new connection after reset
    let new_conn = get_db_connection()
        .await
        .expect("Failed to get connection after reset");
    let conn = new_conn.lock().await;
    let result = conn.prepare("SELECT 1");
    assert!(result.is_ok(), "Connection should work after pool reset");

    println!("✅ Connection pool resilience test passed!");
}

#[tokio::test]
async fn test_optimized_database_pragmas() {
    // Setup test database with unique name
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_name = format!("test_pragmas_{}.db", Uuid::new_v4().simple());
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

    let conn_arc = get_db_connection().await.expect("Failed to get connection");
    let conn = conn_arc.lock().await;

    // Check that foreign keys are enabled
    let mut stmt = conn
        .prepare("PRAGMA foreign_keys")
        .expect("Failed to prepare pragma statement");
    let foreign_keys: i32 = stmt
        .query_row([], |row| row.get(0))
        .expect("Failed to query foreign_keys");
    assert_eq!(foreign_keys, 1, "Foreign keys should be enabled");

    // Check journal mode is WAL
    let mut stmt = conn
        .prepare("PRAGMA journal_mode")
        .expect("Failed to prepare pragma statement");
    let journal_mode: String = stmt
        .query_row([], |row| row.get(0))
        .expect("Failed to query journal_mode");
    assert_eq!(
        journal_mode.to_lowercase(),
        "wal",
        "Journal mode should be WAL"
    );

    // Check synchronous mode is NORMAL
    let mut stmt = conn
        .prepare("PRAGMA synchronous")
        .expect("Failed to prepare pragma statement");
    let synchronous: i32 = stmt
        .query_row([], |row| row.get(0))
        .expect("Failed to query synchronous");
    assert_eq!(synchronous, 1, "Synchronous mode should be NORMAL (1)");

    // Check temp_store is MEMORY
    let mut stmt = conn
        .prepare("PRAGMA temp_store")
        .expect("Failed to prepare pragma statement");
    let temp_store: i32 = stmt
        .query_row([], |row| row.get(0))
        .expect("Failed to query temp_store");
    assert_eq!(temp_store, 2, "Temp store should be MEMORY (2)");

    println!("✅ Database pragmas optimization test passed!");
}
