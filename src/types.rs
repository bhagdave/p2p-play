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

pub type Stories = Vec<Story>;

#[derive(Debug, Serialize, Deserialize)]
pub enum ListMode {
    ALL,
    One(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListRequest {
    mode: ListMode,
}

impl ListRequest {
    pub fn new(mode: ListMode) -> Self {
        Self { mode }
    }

    pub fn mode(&self) -> &ListMode {
        &self.mode
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    mode: ListMode,
    data: Stories,
    receiver: String,
}

impl ListResponse {
    pub fn new(mode: ListMode, data: Stories, receiver: String) -> Self {
        Self {
            mode,
            data,
            receiver,
        }
    }

    pub fn mode(&self) -> &ListMode {
        &self.mode
    }

    pub fn data(&self) -> &Stories {
        &self.data
    }

    pub fn receiver(&self) -> &str {
        &self.receiver
    }
}
