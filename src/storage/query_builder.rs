/// Query builder patterns for complex database operations
use std::collections::HashMap;

/// Fluent query builder for constructing SQL queries
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    select_columns: Vec<String>,
    from_table: Option<String>,
    joins: Vec<String>,
    where_conditions: Vec<String>,
    group_by: Vec<String>,
    having_conditions: Vec<String>,
    order_by: Vec<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl QueryBuilder {
    /// Create a new query builder
    pub fn new() -> Self {
        Self {
            select_columns: Vec::new(),
            from_table: None,
            joins: Vec::new(),
            where_conditions: Vec::new(),
            group_by: Vec::new(),
            having_conditions: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
        }
    }
    
    /// Add SELECT columns
    pub fn select(mut self, columns: &[&str]) -> Self {
        self.select_columns.extend(columns.iter().map(|s| s.to_string()));
        self
    }
    
    /// Add FROM table
    pub fn from(mut self, table: &str) -> Self {
        self.from_table = Some(table.to_string());
        self
    }
    
    /// Add LEFT JOIN
    pub fn left_join(mut self, table: &str, on_condition: &str) -> Self {
        self.joins.push(format!("LEFT JOIN {} ON {}", table, on_condition));
        self
    }
    
    /// Add INNER JOIN  
    pub fn inner_join(mut self, table: &str, on_condition: &str) -> Self {
        self.joins.push(format!("INNER JOIN {} ON {}", table, on_condition));
        self
    }
    
    /// Add WHERE condition
    pub fn where_clause(mut self, condition: &str) -> Self {
        self.where_conditions.push(condition.to_string());
        self
    }
    
    /// Add WHERE condition with AND
    pub fn and_where(mut self, condition: &str) -> Self {
        self.where_conditions.push(condition.to_string());
        self
    }
    
    /// Add GROUP BY columns
    pub fn group_by(mut self, columns: &[&str]) -> Self {
        self.group_by.extend(columns.iter().map(|s| s.to_string()));
        self
    }
    
    /// Add HAVING condition
    pub fn having(mut self, condition: &str) -> Self {
        self.having_conditions.push(condition.to_string());
        self
    }
    
    /// Add ORDER BY
    pub fn order_by(mut self, column: &str, direction: OrderDirection) -> Self {
        self.order_by.push(format!("{} {}", column, direction.as_str()));
        self
    }
    
    /// Add LIMIT
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
    
    /// Add OFFSET
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
    
    /// Build the SQL query string
    pub fn build(self) -> String {
        let mut query = String::new();
        
        // SELECT clause
        if self.select_columns.is_empty() {
            query.push_str("SELECT *");
        } else {
            query.push_str(&format!("SELECT {}", self.select_columns.join(", ")));
        }
        
        // FROM clause
        if let Some(table) = self.from_table {
            query.push_str(&format!(" FROM {}", table));
        }
        
        // JOIN clauses
        for join in self.joins {
            query.push_str(&format!(" {}", join));
        }
        
        // WHERE clause
        if !self.where_conditions.is_empty() {
            query.push_str(&format!(" WHERE {}", self.where_conditions.join(" AND ")));
        }
        
        // GROUP BY clause
        if !self.group_by.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by.join(", ")));
        }
        
        // HAVING clause
        if !self.having_conditions.is_empty() {
            query.push_str(&format!(" HAVING {}", self.having_conditions.join(" AND ")));
        }
        
        // ORDER BY clause
        if !self.order_by.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_by.join(", ")));
        }
        
        // LIMIT clause
        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        
        // OFFSET clause
        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }
        
        query
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Order direction for ORDER BY clauses
#[derive(Debug, Clone, Copy)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl OrderDirection {
    fn as_str(self) -> &'static str {
        match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        }
    }
}

/// Parameter binding helper for prepared statements
#[derive(Debug, Clone)]
pub struct ParameterBinder {
    parameters: HashMap<String, String>,
}

impl ParameterBinder {
    pub fn new() -> Self {
        Self {
            parameters: HashMap::new(),
        }
    }
    
    pub fn bind(mut self, key: &str, value: &str) -> Self {
        self.parameters.insert(key.to_string(), value.to_string());
        self
    }
    
    pub fn get_params_as_vec(&self, keys: &[&str]) -> Vec<String> {
        keys.iter()
            .map(|key| self.parameters.get(*key).cloned().unwrap_or_default())
            .collect()
    }
}

impl Default for ParameterBinder {
    fn default() -> Self {
        Self::new()
    }
}

/// Commonly used query patterns
pub struct CommonQueries;

impl CommonQueries {
    /// Build a query to get stories by channel with optional read status filtering
    pub fn stories_by_channel_with_read_status(_channel: &str, _peer_id: &str) -> QueryBuilder {
        QueryBuilder::new()
            .select(&["s.id", "s.name", "s.header", "s.body", "s.public", "s.channel", "s.created_at"])
            .from("stories s")
            .left_join("story_read_status srs", "s.id = srs.story_id AND srs.peer_id = ?")
            .where_clause("s.channel = ?")
            .and_where("s.public = 1")
            .order_by("s.created_at", OrderDirection::Desc)
    }
    
    /// Build a query to get unread story counts by channel
    pub fn unread_counts_by_channel(_peer_id: &str) -> QueryBuilder {
        QueryBuilder::new()
            .select(&["s.channel", "COUNT(*) as unread_count"])
            .from("stories s")
            .left_join("story_read_status srs", "s.id = srs.story_id AND srs.peer_id = ?")
            .where_clause("s.public = 1")
            .and_where("srs.story_id IS NULL")
            .group_by(&["s.channel"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_query_builder() {
        let query = QueryBuilder::new()
            .select(&["id", "name", "created_at"])
            .from("stories")
            .where_clause("public = 1")
            .order_by("created_at", OrderDirection::Desc)
            .limit(10)
            .build();
            
        assert_eq!(
            query, 
            "SELECT id, name, created_at FROM stories WHERE public = 1 ORDER BY created_at DESC LIMIT 10"
        );
    }
    
    #[test]
    fn test_complex_query_with_joins() {
        let query = QueryBuilder::new()
            .select(&["s.id", "s.name", "COUNT(srs.story_id) as read_count"])
            .from("stories s")
            .left_join("story_read_status srs", "s.id = srs.story_id")
            .where_clause("s.public = 1")
            .group_by(&["s.id", "s.name"])
            .order_by("s.created_at", OrderDirection::Desc)
            .build();
            
        assert!(query.contains("LEFT JOIN"));
        assert!(query.contains("GROUP BY"));
        assert!(query.contains("ORDER BY"));
    }
    
    #[test]
    fn test_parameter_binder() {
        let binder = ParameterBinder::new()
            .bind("channel", "general")
            .bind("peer_id", "test123");
            
        let params = binder.get_params_as_vec(&["peer_id", "channel"]);
        assert_eq!(params, vec!["test123", "general"]);
    }
    
    #[test]
    fn test_common_queries() {
        let query = CommonQueries::unread_counts_by_channel("peer123").build();
        assert!(query.contains("LEFT JOIN"));
        assert!(query.contains("GROUP BY"));
        assert!(query.contains("srs.story_id IS NULL"));
    }
}