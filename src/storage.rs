use rune::{ContextError, Module, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

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
    pub async fn set(&self, key: &str, value: Value) -> Result<(), String> {
        self.db.write().await.insert(
            key.to_owned(),
            serde_json::to_value(value).map_err(|e| e.to_string())?,
        );
        Ok(())
    }

    /// Retrieve a value by key
    pub async fn get(&self, key: &str) -> Result<Option<Value>, String> {
        Ok(self
            .db
            .read()
            .await
            .get(key)
            .cloned()
            .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
            .transpose()?)
    }

    /// Delete a value by key
    pub async fn delete(&self, key: &str) -> Result<bool, String> {
        Ok(self.db.write().await.remove(key).is_some())
    }

    /// Check if a key exists
    pub async fn has(&self, key: &str) -> Result<bool, String> {
        Ok(self.db.read().await.contains_key(key))
    }

    /// Clear all values from storage
    pub async fn clear(&self) -> Result<(), String> {
        Ok(self.db.write().await.clear())
    }

    /// Get all keys in storage
    pub async fn keys(&self) -> Result<Vec<String>, String> {
        let db = self.db.read().await;
        Ok(db.keys().cloned().collect::<Vec<String>>())
    }

    /// Get the number of keys in storage
    pub async fn len(&self) -> Result<usize, String> {
        Ok(self.db.read().await.len())
    }

    /// Check if storage is empty
    pub async fn is_empty(&self) -> Result<bool, String> {
        Ok(self.db.read().await.is_empty())
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
                tokio::runtime::Handle::current().block_on(storage.set(key, value))
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("get", move |key: &str| {
                tokio::runtime::Handle::current().block_on(storage.get(key))
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("delete", move |key: &str| {
                tokio::runtime::Handle::current().block_on(storage.delete(key))
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("has", move |key: &str| {
                tokio::runtime::Handle::current().block_on(storage.has(key))
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("clear", move || {
                tokio::runtime::Handle::current().block_on(storage.clear())
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("keys", move || {
                tokio::runtime::Handle::current().block_on(storage.keys())
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("len", move || {
                tokio::runtime::Handle::current().block_on(storage.len())
            })
            .build()?;
    }
    {
        let storage = storage.clone();
        module
            .function("is_empty", move || {
                tokio::runtime::Handle::current().block_on(storage.is_empty())
            })
            .build()?;
    }

    Ok(module)
}
