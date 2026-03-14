use rune::{ContextError, Module, Value};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// A thread-safe persistent key-value store
#[derive(Clone)]
pub struct Storage {
    db: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl Storage {
    /// Create a new storage instance with the given database path
    pub fn new() -> Self {
        let db = HashMap::new();
        Self {
            db: Arc::new(RwLock::new(db)),
        }
    }

    /// Store a value with the given key
    pub fn set(&self, key: &str, value: Value) -> Result<(), String> {
        self.db.write().map_err(|e| e.to_string())?.insert(
            key.to_owned(),
            serde_json::to_value(value).map_err(|e| e.to_string())?,
        );
        Ok(())
    }

    /// Retrieve a value by key
    pub fn get(&self, key: &str) -> Result<Option<Value>, String> {
        Ok(self
            .db
            .read()
            .map_err(|e| e.to_string())?
            .get(key)
            .cloned()
            .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
            .transpose()?)
    }

    /// Delete a value by key
    pub fn delete(&self, key: &str) -> Result<bool, String> {
        Ok(self
            .db
            .write()
            .map_err(|e| e.to_string())?
            .remove(key)
            .is_some())
    }

    /// Check if a key exists
    pub fn has(&self, key: &str) -> Result<bool, String> {
        Ok(self.db.read().map_err(|e| e.to_string())?.contains_key(key))
    }

    /// Clear all values from storage
    pub fn clear(&self) -> Result<(), String> {
        Ok(self.db.write().map_err(|e| e.to_string())?.clear())
    }

    /// Get all keys in storage
    pub fn keys(&self) -> Result<Vec<String>, String> {
        let db = self.db.read().map_err(|e| e.to_string())?;
        Ok(db.keys().cloned().collect::<Vec<String>>())
    }

    /// Get the number of keys in storage
    pub fn len(&self) -> Result<usize, String> {
        Ok(self.db.read().map_err(|e| e.to_string())?.len())
    }

    /// Check if storage is empty
    pub fn is_empty(&self) -> Result<bool, String> {
        Ok(self.db.read().map_err(|e| e.to_string())?.is_empty())
    }
}

/// Create a Rune module for the storage functionality
pub fn create_storage_module(storage: &Storage) -> Result<Module, ContextError> {
    let mut module = Module::with_item(["storage"])?;

    // Register functions
    {
        let storage = storage.clone();
        module
            .function("set", move |key: &str, value: Value| {
                storage.set(key, value)
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("get", move |key: &str| storage.get(key))
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("delete", move |key: &str| storage.delete(key))
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("has", move |key: &str| storage.has(key))
            .build()?;
    }
    {
        let storage = storage.clone();
        module.function("clear", move || storage.clear()).build()?;
    }
    {
        let storage = storage.clone();
        module.function("keys", move || storage.keys()).build()?;
    }
    {
        let storage = storage.clone();
        module.function("len", move || storage.len()).build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("is_empty", move || storage.is_empty())
            .build()?;
    }

    Ok(module)
}
