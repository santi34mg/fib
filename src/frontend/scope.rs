use std::collections::HashMap;

use crate::frontend::{scope_kind::ScopeKind, tokens::name::Name};

#[derive(Debug, Clone)]
pub struct Scope {
    kind: ScopeKind,
    name_map: HashMap<String, Name>,
    // TODO: define frame correctly
    frame: HashMap<Name, Option<()>>,
}

impl Scope {
    pub fn new(kind: ScopeKind) -> Self {
        Self {
            kind,
            name_map: HashMap::new(),
            frame: HashMap::new(),
        }
    }

    pub fn insert_name(&mut self, name: &mut Name) {
        if self.name_map.contains_key(&name.name) {
            name.id = self
                .name_map
                .get(&name.name)
                // this is never None because we just checked
                .unwrap()
                .id;
        } else {
            let name_str = name.name.clone();
            // this is always None
            self.name_map.insert(name_str, name.clone());
            // nothing to update on the symbol
        }
    }
}
