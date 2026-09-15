use std::collections::HashMap;

use crate::semantic::types::QualType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Clone, PartialEq, Debug)]
pub enum InitType {
    Declaration,
    Definition,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub qtype: QualType,
    pub kind: InitType,
}

pub struct Scope {
    symbols: HashMap<String, Symbol>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }
}

pub struct ScopeTree {
    scopes: Vec<Scope>,
    next_symbol_id: usize,
}

impl ScopeTree {
    pub fn new() -> Self {
        // initialize with a global scope
        Self {
            scopes: vec![Scope::new()],
            next_symbol_id: 0,
        }
    }

    pub fn enter(&mut self) {
        self.scopes.push(Scope::new());
    }

    pub fn is_global(&self) -> bool {
        self.scopes.len() == 1
    }

    pub fn exit(&mut self) {
        self.scopes.pop();
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.symbols.get(name) {
                return Some(symbol);
            }
        }
        None
    }

    pub fn lookup_local(&self, name: &str) -> Option<&Symbol> {
        if let Some(scope) = self.scopes.last() {
            return scope.symbols.get(name);
        }
        None
    }

    pub fn new_symbol(&mut self, name: String, qtype: QualType, kind: InitType) -> Symbol {
        let id = SymbolId(self.next_symbol_id);
        self.next_symbol_id += 1;

        Symbol {
            id,
            name,
            qtype,
            kind,
        }
    }

    pub fn define(&mut self, name: String, symbol: Symbol) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.symbols.insert(name, symbol);
        }
    }

    pub fn check_redefinition(&self, new: &Symbol, existing: &Symbol) -> bool {
        if !new.qtype.type_compatible(&existing.qtype) {
            return true;
        }

        matches!(
            (&new.kind, &existing.kind),
            (InitType::Definition, InitType::Definition)
        )
    }
}
