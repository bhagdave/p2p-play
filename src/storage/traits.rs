/// Generic traits for database operations to reduce code duplication
use rusqlite::Connection;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Trait for database operations that can be executed with a connection
#[async_trait::async_trait]
pub trait DatabaseOperation<T> {
    type Error: Error + Send + Sync + 'static;

    /// Execute the operation with a database connection
    async fn execute(&self, conn: &Arc<Mutex<Connection>>) -> Result<T, Self::Error>;
}

/// Generic function to execute a database operation
pub async fn execute_db_operation<T, Op>(
    get_connection: impl Fn() -> Result<Arc<Mutex<Connection>>, Box<dyn Error>> + Send,
    operation: Op,
) -> Result<T, Box<dyn Error>>
where
    Op: DatabaseOperation<T>,
    Op::Error: 'static,
{
    let conn = get_connection()?;
    let result = operation.execute(&conn).await?;
    Ok(result)
}

/// Trait for configuration objects that can be saved and loaded
pub trait ConfigStorage<T>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + Default + Clone,
{
    /// Validate the configuration
    fn validate(&self) -> Result<(), Box<dyn Error>>;

    /// Get the default file path for this config type
    fn default_file_path() -> &'static str;
}

/// Generic configuration save operation
pub async fn save_config_to_path<T>(config: &T, path: &str) -> Result<(), Box<dyn Error>>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + ConfigStorage<T> + Default + Clone,
{
    config.validate()?;
    let json = serde_json::to_string_pretty(config)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}

/// Generic configuration load operation
pub async fn load_config_from_path<T>(path: &str) -> Result<T, Box<dyn Error>>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + ConfigStorage<T> + Default + Clone,
{
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            let config: T = serde_json::from_str(&content)?;
            config.validate()?;
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let default_config = T::default();
            save_config_to_path(&default_config, path).await?;
            Ok(default_config)
        }
        Err(e) => Err(e.into()),
    }
}

/// Generic function to ensure config file exists
pub async fn ensure_config_exists<T>() -> Result<(), Box<dyn Error>>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + ConfigStorage<T> + Default + Clone,
{
    let path = T::default_file_path();
    if tokio::fs::metadata(path).await.is_err() {
        let default_config = T::default();
        save_config_to_path(&default_config, path).await?;
    }
    Ok(())
}

/// Trait for common query patterns
pub trait QueryPattern {
    fn build_select_all(&self) -> String;
    fn build_select_by_id(&self, table: &str) -> String;
    fn build_insert(&self, table: &str, columns: &[&str]) -> String;
    fn build_update(&self, table: &str, columns: &[&str], where_clause: &str) -> String;
    fn build_delete(&self, table: &str, where_clause: &str) -> String;
}

/// Standard query builder implementation
pub struct StandardQueryBuilder;

impl QueryPattern for StandardQueryBuilder {
    fn build_select_all(&self) -> String {
        "SELECT * FROM {} ORDER BY created_at DESC".to_string()
    }

    fn build_select_by_id(&self, table: &str) -> String {
        format!("SELECT * FROM {table} WHERE id = ?")
    }

    fn build_insert(&self, table: &str, columns: &[&str]) -> String {
        let placeholders = columns.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders
        )
    }

    fn build_update(&self, table: &str, columns: &[&str], where_clause: &str) -> String {
        let set_clause = columns
            .iter()
            .map(|col| format!("{col} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("UPDATE {table} SET {set_clause} WHERE {where_clause}")
    }

    fn build_delete(&self, table: &str, where_clause: &str) -> String {
        format!("DELETE FROM {table} WHERE {where_clause}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let builder = StandardQueryBuilder;

        assert_eq!(
            builder.build_select_by_id("stories"),
            "SELECT * FROM stories WHERE id = ?"
        );

        assert_eq!(
            builder.build_insert("stories", &["name", "header", "body"]),
            "INSERT INTO stories (name, header, body) VALUES (?, ?, ?)"
        );

        assert_eq!(
            builder.build_update("stories", &["name", "public"], "id = ?"),
            "UPDATE stories SET name = ?, public = ? WHERE id = ?"
        );

        assert_eq!(
            builder.build_delete("stories", "id = ?"),
            "DELETE FROM stories WHERE id = ?"
        );
    }
}
