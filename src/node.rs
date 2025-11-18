use std::collections::HashMap;
use serde::Serialize;
use serde_repr::Serialize_repr;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OscNode {
    #[serde(rename = "FULL_PATH")]
    pub full_path: String,

    #[serde(rename = "ACCESS", skip_serializing_if = "Option::is_none")]
    pub access: Option<Access>,

    /// TYPE: standard OSC typetag string, e.g. "f", "i", "s" etc.
    #[serde(rename = "TYPE", skip_serializing_if = "Option::is_none")]
    pub typetag: Option<String>,

    #[serde(rename = "VALUE", skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,

    #[serde(rename = "CONTENTS", skip_serializing_if = "HashMap::is_empty")]
    pub contents: HashMap<String, OscNode>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize_repr)]
pub enum Access {
    None = 0,
    Read = 1,
    Write = 2,
    ReadWrite = 3,
}

impl OscNode {
    pub fn new_container(full_path: &str) -> Self {
        Self {
            full_path: full_path.to_string(),
            access: Some(Access::None),
            typetag: None,
            value: None,
            contents: HashMap::new(),
        }
    }

    pub fn new_method(full_path: &str, access: Access, typetag: &str) -> Self {
        Self {
            full_path: full_path.to_string(),
            access: Some(access),
            typetag: Some(typetag.to_string()),
            value: None,
            contents: HashMap::new(),
        }
    }

    pub fn ensure_path<'a>(root: &'a mut OscNode, path: &str) -> &'a mut OscNode {
        if path == "/" {
            return root;
        }

        let mut parts = path.trim_matches('/').split('/');
        let mut current = root;

        let mut base = String::new();
        for part in parts.by_ref() {
            base.push('/');
            base.push_str(part);
            let key = part.to_string();
            current = current
                .contents
                .entry(key.clone())
                .or_insert_with(|| OscNode::new_container(&base));
        }
        current
    }

    pub fn add_method(root: &mut OscNode, path: &str, access: Access, typetag: &str) {
        let parent_path = match path.rfind('/') {
            Some(idx) if idx > 0 => &path[..idx],
            _ => "/",
        };
        let name = path_name(path).unwrap_or_else(|| path.trim_matches('/').to_string());

        let parent = Self::ensure_path(root, parent_path);
        parent.contents.insert(
            name.clone(),
            OscNode::new_method(path, access, typetag),
        );
    }
}

fn path_name(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }
    path.trim_matches('/')
        .rsplit('/')
        .next()
        .map(|s| s.to_string())
}
