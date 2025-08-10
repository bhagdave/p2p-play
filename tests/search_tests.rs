/// Integration tests for search functionality
use p2p_play::storage::{
    create_new_story_with_channel, filter_stories_by_channel, filter_stories_by_recent_days,
    search_stories, init_test_database,
};
use p2p_play::types::SearchQuery;
use std::env;
use uuid::Uuid;

#[tokio::test]
async fn test_search_functionality() {
    // Set up unique test database
    let unique_id = Uuid::new_v4().to_string().replace('-', "_");
    let db_path = format!("/tmp/test_search_{}.db", unique_id);
    unsafe {
        env::set_var("TEST_DATABASE_PATH", &db_path);
    }

    // Initialize test database
    init_test_database().await.expect("Failed to init test database");

    // Create test stories
    create_new_story_with_channel(
        "Rust Programming Tutorial",
        "Learn the basics",
        "This story teaches you how to program in Rust. It covers ownership, borrowing, and lifetimes.",
        "tech"
    ).await.expect("Failed to create story 1");

    create_new_story_with_channel(
        "Cooking Adventures",
        "Delicious recipes",
        "A collection of easy recipes for beginners. Learn to cook pasta, rice, and simple desserts.",
        "lifestyle"
    ).await.expect("Failed to create story 2");

    create_new_story_with_channel(
        "Programming Best Practices",
        "Clean code tips",
        "This story covers programming principles, code review practices, and software architecture.",
        "tech"
    ).await.expect("Failed to create story 3");

    // Test 1: Basic text search
    let query = SearchQuery::new("programming".to_string());
    let results = search_stories(&query).await.expect("Search failed");
    assert_eq!(results.len(), 2, "Should find 2 stories with 'programming'");

    // Test 2: Channel filter
    let query = SearchQuery::new("".to_string()).with_channel("tech".to_string());
    let results = search_stories(&query).await.expect("Channel filter failed");
    assert_eq!(results.len(), 2, "Should find 2 stories in 'tech' channel");

    // Test 3: Combined text and channel search
    let query = SearchQuery::new("rust".to_string()).with_channel("tech".to_string());
    let results = search_stories(&query).await.expect("Combined search failed");
    assert_eq!(results.len(), 1, "Should find 1 story with 'rust' in 'tech' channel");

    // Test 4: Filter by channel function
    let stories = filter_stories_by_channel("lifestyle").await.expect("Channel filter failed");
    assert_eq!(stories.len(), 1, "Should find 1 story in 'lifestyle' channel");

    // Test 5: Filter by recent days (all stories should be recent)
    let stories = filter_stories_by_recent_days(1).await.expect("Recent filter failed");
    assert_eq!(stories.len(), 3, "Should find all 3 stories from today");

    // Test 6: No results case
    let query = SearchQuery::new("nonexistent".to_string());
    let results = search_stories(&query).await.expect("Search failed");
    assert_eq!(results.len(), 0, "Should find no stories with 'nonexistent'");

    // Test 7: Relevance scoring
    let query = SearchQuery::new("programming".to_string());
    let results = search_stories(&query).await.expect("Search failed");
    for result in &results {
        assert!(result.relevance_score.is_some(), "Should have relevance score for text search");
        assert!(result.relevance_score.unwrap() > 0.0, "Relevance score should be positive");
    }

    println!("✅ All search functionality tests passed!");
}

#[tokio::test]
async fn test_search_edge_cases() {
    // Set up unique test database
    let unique_id = Uuid::new_v4().to_string().replace('-', "_");
    let db_path = format!("/tmp/test_search_edge_{}.db", unique_id);
    unsafe {
        env::set_var("TEST_DATABASE_PATH", &db_path);
    }

    // Initialize test database
    init_test_database().await.expect("Failed to init test database");

    // Test empty query
    let query = SearchQuery::new("".to_string());
    assert!(query.is_empty(), "Empty query should be detected");

    // Test search with no stories in database
    let results = search_stories(&query).await.expect("Search failed");
    assert_eq!(results.len(), 0, "Should find no stories in empty database");

    // Create a story and test case sensitivity
    create_new_story_with_channel(
        "Test Story",
        "Test Header",
        "This is a test story with UPPERCASE and lowercase words.",
        "general"
    ).await.expect("Failed to create test story");

    // Test case insensitive search
    let query = SearchQuery::new("UPPERCASE".to_string());
    let results = search_stories(&query).await.expect("Search failed");
    assert_eq!(results.len(), 1, "Should find story with case insensitive search");

    let query = SearchQuery::new("uppercase".to_string());
    let results = search_stories(&query).await.expect("Search failed");
    assert_eq!(results.len(), 1, "Should find story with case insensitive search");

    println!("✅ All edge case tests passed!");
}