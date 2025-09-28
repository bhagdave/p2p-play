use std::collections::HashMap;

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

    pub fn select(mut self, columns: &[&str]) -> Self {
        self.select_columns
            .extend(columns.iter().map(|s| s.to_string()));
        self
    }

    pub fn from(mut self, table: &str) -> Self {
        self.from_table = Some(table.to_string());
        self
    }

    pub fn left_join(mut self, table: &str, on_condition: &str) -> Self {
        self.joins
            .push(format!("LEFT JOIN {table} ON {on_condition}"));
        self
    }

    pub fn inner_join(mut self, table: &str, on_condition: &str) -> Self {
        self.joins
            .push(format!("INNER JOIN {table} ON {on_condition}"));
        self
    }

    pub fn where_clause(mut self, condition: &str) -> Self {
        self.where_conditions.push(condition.to_string());
        self
    }

    pub fn and_where(mut self, condition: &str) -> Self {
        self.where_conditions.push(condition.to_string());
        self
    }

    pub fn group_by(mut self, columns: &[&str]) -> Self {
        self.group_by.extend(columns.iter().map(|s| s.to_string()));
        self
    }

    pub fn having(mut self, condition: &str) -> Self {
        self.having_conditions.push(condition.to_string());
        self
    }

    pub fn order_by(mut self, column: &str, direction: OrderDirection) -> Self {
        self.order_by
            .push(format!("{} {}", column, direction.as_str()));
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn build(self) -> String {
        let mut query = String::new();

        if self.select_columns.is_empty() {
            query.push_str("SELECT *");
        } else {
            query.push_str(&format!("SELECT {}", self.select_columns.join(", ")));
        }

        if let Some(table) = self.from_table {
            query.push_str(&format!(" FROM {table}"));
        }

        for join in self.joins {
            query.push_str(&format!(" {join}"));
        }

        if !self.where_conditions.is_empty() {
            query.push_str(&format!(" WHERE {}", self.where_conditions.join(" AND ")));
        }

        if !self.group_by.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_by.join(", ")));
        }

        if !self.having_conditions.is_empty() {
            query.push_str(&format!(" HAVING {}", self.having_conditions.join(" AND ")));
        }

        if !self.order_by.is_empty() {
            query.push_str(&format!(" ORDER BY {}", self.order_by.join(", ")));
        }

        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {limit}"));
        }

        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {offset}"));
        }

        query
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

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
}
