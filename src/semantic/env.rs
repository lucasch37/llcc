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

#[derive(Clone, Debug, Default)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
}

impl SymbolTable {
    pub fn insert(&mut self, name: String, qtype: QualType, kind: InitType) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol {
            id,
            name,
            qtype,
            kind,
        });
        id
    }

    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.0)
    }

    pub fn get_mut(&mut self, id: SymbolId) -> Option<&mut Symbol> {
        self.symbols.get_mut(id.0)
    }
}

pub struct Scope {
    symbols: HashMap<String, SymbolId>,
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
    symbols: SymbolTable,
}

impl ScopeTree {
    pub fn new() -> Self {
        // initialize with a global scope
        Self {
            scopes: vec![Scope::new()],
            symbols: SymbolTable::default(),
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

    pub fn lookup(&self, name: &str) -> Option<SymbolId> {
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.symbols.get(name) {
                return Some(*id);
            }
        }
        None
    }

    pub fn lookup_local(&self, name: &str) -> Option<SymbolId> {
        if let Some(scope) = self.scopes.last() {
            return scope.symbols.get(name).copied();
        }
        None
    }

    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        self.symbols.get(id).expect("unknown symbol id")
    }

    pub fn symbol_mut(&mut self, id: SymbolId) -> &mut Symbol {
        self.symbols.get_mut(id).expect("unknown symbol id")
    }

    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    pub fn new_symbol(&mut self, name: String, qtype: QualType, kind: InitType) -> SymbolId {
        self.symbols.insert(name, qtype, kind)
    }

    pub fn define(&mut self, name: String, id: SymbolId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.symbols.insert(name, id);
        }
    }

    pub fn check_redefinition(
        &self,
        qtype: &QualType,
        kind: &InitType,
        existing: SymbolId,
    ) -> bool {
        let existing = self.symbol(existing);

        if !qtype.type_compatible(&existing.qtype) {
            return true;
        }

        matches!(
            (kind, &existing.kind),
            (InitType::Definition, InitType::Definition)
        )
    }
}
