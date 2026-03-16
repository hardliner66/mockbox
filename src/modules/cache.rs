use rugen::rune::{ContextError, Module, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// A thread-safe persistent key-value store
#[derive(Clone)]
pub struct Cache {
    db: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl Cache {
    /// Create a new cache instance
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
        self.db
            .read()
            .await
            .get(key)
            .cloned()
            .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
            .transpose()
    }

    /// Delete a value by key
    pub async fn delete(&self, key: &str) -> Result<bool, String> {
        Ok(self.db.write().await.remove(key).is_some())
    }

    /// Check if a key exists
    pub async fn has(&self, key: &str) -> Result<bool, String> {
        Ok(self.db.read().await.contains_key(key))
    }

    /// Clear all values from cache
    pub async fn clear(&self) -> Result<(), String> {
        self.db.write().await.clear();
        Ok(())
    }

    /// Get all keys in cache
    pub async fn keys(&self) -> Result<Vec<String>, String> {
        let db = self.db.read().await;
        Ok(db.keys().cloned().collect::<Vec<String>>())
    }

    /// Get the number of keys in cache
    pub async fn len(&self) -> Result<usize, String> {
        Ok(self.db.read().await.len())
    }

    /// Check if cache is empty
    pub async fn is_empty(&self) -> Result<bool, String> {
        Ok(self.db.read().await.is_empty())
    }
}

/// Create a Rune module for the cache functionality
pub fn cache_module(cache: &Cache) -> Result<Module, ContextError> {
    let mut module = Module::with_item(["cache"])?;

    // Register functions
    {
        let cache = cache.clone();
        module
            .function("set", move |key: &str, value: Value| {
                tokio::runtime::Handle::current().block_on(cache.set(key, value))
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("get", move |key: &str| {
                tokio::runtime::Handle::current().block_on(cache.get(key))
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("delete", move |key: &str| {
                tokio::runtime::Handle::current().block_on(cache.delete(key))
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("has", move |key: &str| {
                tokio::runtime::Handle::current().block_on(cache.has(key))
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("clear", move || {
                tokio::runtime::Handle::current().block_on(cache.clear())
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("keys", move || {
                tokio::runtime::Handle::current().block_on(cache.keys())
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("len", move || {
                tokio::runtime::Handle::current().block_on(cache.len())
            })
            .build()?;
    }
    {
        let cache = cache.clone();
        module
            .function("is_empty", move || {
                tokio::runtime::Handle::current().block_on(cache.is_empty())
            })
            .build()?;
    }

    Ok(module)
}
