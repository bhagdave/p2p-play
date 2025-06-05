use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Story {
    id: usize,
    name: String,
    header: String,
    body: String,
    public: bool,
}

impl Story {
    pub fn new(id: usize, name: String, header: String, body: String, public: bool) -> Self {
        Self {
            id,
            name,
            header,
            body,
            public,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_id(&mut self, id: usize) {
        self.id = id;
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn header(&self) -> &str {
        &self.header
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn is_public(&self) -> bool {
        self.public
    }
    pub fn set_public(&mut self, public: bool) {
        self.public = public;
    }
}
