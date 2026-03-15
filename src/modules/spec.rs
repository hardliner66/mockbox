use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use parking_lot::RwLock;
use rand::{RngExt, seq::IndexedRandom};
use rune::{
    Any, ContextError, Module, Value,
    alloc::{Result, String as RuneString},
    runtime::{Object, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RuntimeError},
};

use crate::helper::to_string;

#[derive(Clone, Copy)]
enum TypeCategory {
    UInt,
    Int,
    Float,
    Char,
}

static TYPE_CATEGORIES: LazyLock<Arc<RwLock<HashMap<String, TypeCategory>>>> =
    LazyLock::new(|| {
        let mut hm = HashMap::new();
        hm.insert(
            rune::to_value(0u8).unwrap().type_hash().to_string(),
            TypeCategory::UInt,
        );
        hm.insert(
            rune::to_value(0u16).unwrap().type_hash().to_string(),
            TypeCategory::UInt,
        );
        hm.insert(
            rune::to_value(0u32).unwrap().type_hash().to_string(),
            TypeCategory::UInt,
        );
        hm.insert(
            rune::to_value(0u64).unwrap().type_hash().to_string(),
            TypeCategory::UInt,
        );
        hm.insert(
            rune::to_value(0i8).unwrap().type_hash().to_string(),
            TypeCategory::Int,
        );
        hm.insert(
            rune::to_value(0i16).unwrap().type_hash().to_string(),
            TypeCategory::Int,
        );
        hm.insert(
            rune::to_value(0i32).unwrap().type_hash().to_string(),
            TypeCategory::Int,
        );
        hm.insert(
            rune::to_value(0i64).unwrap().type_hash().to_string(),
            TypeCategory::Int,
        );
        hm.insert(
            rune::to_value(0f32).unwrap().type_hash().to_string(),
            TypeCategory::Float,
        );
        hm.insert(
            rune::to_value(0f64).unwrap().type_hash().to_string(),
            TypeCategory::Float,
        );
        hm.insert(
            rune::to_value(char::default())
                .unwrap()
                .type_hash()
                .to_string(),
            TypeCategory::Char,
        );
        Arc::new(RwLock::new(hm))
    });

#[derive(Any, Debug)]
enum Spec {
    Bool,
    Just(Value),

    UInt {
        min: u128,
        max: u128,
    },
    Int {
        min: i128,
        max: i128,
    },
    Char {
        min: char,
        max: char,
    },

    Float {
        min: f64,
        max: f64,
    },

    String {
        len: Box<Spec>,
        min: char,
        max: char,
    },
    AlphaNumeric {
        len: Box<Spec>,
    },

    OneOf(Vec<Spec>),
    Weighted(Vec<(u32, Spec)>),
    Array {
        len: Box<Spec>,
        item: Box<Spec>,
    },
    Object(Object),
    Optional {
        p: Box<Spec>,
        item: Box<Spec>,
    },
    Tuple(Vec<Spec>),
    Error(String),
}

impl Default for Spec {
    fn default() -> Self {
        Spec::Error("Unsupported Type".to_string())
    }
}

#[rune::function]
fn just(value: Value) -> Spec {
    Spec::Just(value)
}

#[rune::function]
fn literal(value: Value) -> Spec {
    Spec::Just(value)
}

#[rune::function]
fn bool() -> Spec {
    Spec::Bool
}

fn range_impl(min: &Value, max: &Value) -> Spec {
    match min {
        min if min.as_integer::<u128>().is_ok() => {
            let Ok(min) = min.as_integer::<u128>() else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(max) = max.as_integer::<u128>() else {
                return Spec::Error("Invalid range end".to_string());
            };
            Spec::UInt { min, max }
        }
        min if min.as_integer::<i128>().is_ok() => {
            let Ok(min) = min.as_integer::<i128>() else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(max) = max.as_integer::<i128>() else {
                return Spec::Error("Invalid range end".to_string());
            };
            Spec::Int { min, max }
        }
        min if min.as_float().is_ok() => {
            let Ok(min) = min.as_float() else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(max) = max.as_float() else {
                return Spec::Error("Invalid range end".to_string());
            };
            Spec::Float { min, max }
        }
        min if min.as_char().is_ok() => {
            let Ok(min) = min.as_char() else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(max) = max.as_char() else {
                return Spec::Error("Invalid range end".to_string());
            };
            Spec::Char { min, max }
        }
        _ => Spec::Error("Unsupported type".to_string()),
    }
}

#[rune::function]
#[expect(clippy::needless_pass_by_value)]
fn range(min: Value, max: Value) -> Spec {
    range_impl(&min, &max)
}

#[rune::function]
fn char(min: char, max: char) -> Spec {
    Spec::Char { min, max }
}

#[rune::function]
fn uint(min: u128, max: u128) -> Spec {
    Spec::UInt { min, max }
}

#[rune::function]
fn int(min: i128, max: i128) -> Spec {
    Spec::Int { min, max }
}

#[rune::function]
fn float(min: f64, max: f64) -> Spec {
    Spec::Float { min, max }
}

impl From<&Value> for Spec {
    fn from(value: &Value) -> Self {
        if let Ok(spec) = rune::from_value::<Spec>(value) {
            spec
        } else if let Ok(s) = rune::from_value::<Object>(value) {
            Spec::Object(s)
        } else if let Ok(s) = rune::from_value::<Range>(value) {
            let Ok(start) = rune::to_value(s.start) else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(end) = rune::to_value(s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&start, &end)
        } else if let Ok(s) = rune::from_value::<RangeInclusive>(value) {
            let Ok(start) = rune::to_value(s.start) else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(end) = rune::to_value(s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&start, &end)
        } else if let Ok(s) = rune::from_value::<RangeFrom>(value) {
            let Ok(start) = rune::to_value(s.start) else {
                return Spec::Error("Invalid range start".to_string());
            };
            let max = match TYPE_CATEGORIES
                .read()
                .get(&start.type_hash().to_string())
                .copied()
            {
                Some(TypeCategory::UInt) => rune::to_value(u64::MAX),
                Some(TypeCategory::Int) => rune::to_value(i64::MAX),
                Some(TypeCategory::Float) => rune::to_value(f64::MAX),
                Some(TypeCategory::Char) => rune::to_value(char::MAX),
                _ => return Spec::Error("Unsupported type".to_string()),
            };
            let Ok(max) = max else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&start, &max)
        } else if let Ok(s) = rune::from_value::<RangeTo>(value) {
            let Ok(end) = &rune::to_value(&s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            let min = match TYPE_CATEGORIES
                .read()
                .get(&end.type_hash().to_string())
                .copied()
            {
                Some(TypeCategory::UInt) => rune::to_value(u64::MIN),
                Some(TypeCategory::Int) => rune::to_value(i64::MIN),
                Some(TypeCategory::Float) => rune::to_value(f64::MIN),
                Some(TypeCategory::Char) => rune::to_value(char::MIN),
                _ => return Spec::Error("Unsupported type".to_string()),
            };
            let Ok(min) = min else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(end) = rune::to_value(s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&min, &end)
        } else if rune::from_value::<RangeFull>(value).is_ok() {
            Spec::Error("Unsupported type: RangeFull".to_string())
        } else if let Ok(s) = rune::from_value::<Vec<Value>>(value) {
            Spec::Tuple(s.into_iter().map(Into::into).collect())
        } else {
            Spec::Just(value.to_owned())
        }
    }
}

impl From<Value> for Spec {
    fn from(value: Value) -> Self {
        if let Ok(spec) = rune::from_value::<Spec>(&value) {
            spec
        } else if let Ok(s) = rune::from_value::<Object>(&value) {
            Spec::Object(s)
        } else if let Ok(s) = rune::from_value::<Range>(&value) {
            let Ok(start) = rune::to_value(s.start) else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(end) = rune::to_value(s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&start, &end)
        } else if let Ok(s) = rune::from_value::<RangeInclusive>(&value) {
            let Ok(start) = rune::to_value(s.start) else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(end) = rune::to_value(s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&start, &end)
        } else if let Ok(s) = rune::from_value::<RangeFrom>(&value) {
            let Ok(start) = rune::to_value(s.start) else {
                return Spec::Error("Invalid range start".to_string());
            };
            let max = match TYPE_CATEGORIES
                .read()
                .get(&start.type_hash().to_string())
                .copied()
            {
                Some(TypeCategory::UInt) => rune::to_value(u64::MAX),
                Some(TypeCategory::Int) => rune::to_value(i64::MAX),
                Some(TypeCategory::Float) => rune::to_value(f64::MAX),
                Some(TypeCategory::Char) => rune::to_value(char::MAX),
                _ => return Spec::Error("Unsupported type".to_string()),
            };
            let Ok(max) = max else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&start, &max)
        } else if let Ok(s) = rune::from_value::<RangeTo>(&value) {
            let Ok(end) = &rune::to_value(&s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            let min = match TYPE_CATEGORIES
                .read()
                .get(&end.type_hash().to_string())
                .copied()
            {
                Some(TypeCategory::UInt) => rune::to_value(u64::MIN),
                Some(TypeCategory::Int) => rune::to_value(i64::MIN),
                Some(TypeCategory::Float) => rune::to_value(f64::MIN),
                Some(TypeCategory::Char) => rune::to_value(char::MIN),
                _ => return Spec::Error("Unsupported type".to_string()),
            };
            let Ok(min) = min else {
                return Spec::Error("Invalid range start".to_string());
            };
            let Ok(end) = rune::to_value(s.end) else {
                return Spec::Error("Invalid range end".to_string());
            };
            range_impl(&min, &end)
        } else if rune::from_value::<RangeFull>(&value).is_ok() {
            Spec::Error("Unsupported type: RangeFull".to_string())
        } else if let Ok(s) = rune::from_value::<Vec<Value>>(&value) {
            Spec::Tuple(s.into_iter().map(Into::into).collect())
        } else {
            Spec::Just(value)
        }
    }
}

#[rune::function]
fn alphanumeric(len: Value) -> Spec {
    Spec::AlphaNumeric {
        len: Box::new(len.into()),
    }
}

#[rune::function]
fn string(len: Value, min: char, max: char) -> Spec {
    Spec::String {
        len: Box::new(len.into()),
        min,
        max,
    }
}

#[rune::function]
fn one_of(values: Vec<Value>) -> Spec {
    Spec::OneOf(values.into_iter().map(Into::into).collect())
}

#[rune::function(path = choose)]
fn choose_one(values: Vec<Value>) -> Spec {
    Spec::OneOf(values.into_iter().map(Into::into).collect())
}

#[rune::function(instance)]
fn values(count: i64, value: Value) -> Spec {
    let Ok(count) = rune::to_value(count) else {
        return Spec::Error("Count must be a non-negative integer".to_string());
    };
    Spec::Array {
        len: Box::new(Spec::Just(count)),
        item: Box::new(value.into()),
    }
}

#[rune::function(instance)]
fn choose(values: Vec<Value>) -> Spec {
    Spec::OneOf(values.into_iter().map(Into::into).collect())
}

#[rune::function(instance)]
fn pick(values: Vec<Value>) -> Spec {
    Spec::OneOf(values.into_iter().map(Into::into).collect())
}

#[rune::function(path = pick)]
fn pick_one(values: Vec<Value>) -> Spec {
    Spec::OneOf(values.into_iter().map(Into::into).collect())
}

#[rune::function]
fn weighted(values: Vec<(u32, Value)>) -> Spec {
    Spec::Weighted(values.into_iter().map(|(w, v)| (w, v.into())).collect())
}

#[rune::function]
fn array(len: Value, item: Value) -> Spec {
    Spec::Array {
        len: Box::new(len.into()),
        item: Box::new(item.into()),
    }
}

#[rune::function]
fn object(obj: Object) -> Spec {
    Spec::Object(obj)
}

#[rune::function]
fn optional(p: Value, item: Value) -> Spec {
    Spec::Optional {
        p: Box::new(p.into()),
        item: Box::new(item.into()),
    }
}

#[rune::function]
fn tuple(items: Vec<Value>) -> Spec {
    Spec::Tuple(items.into_iter().map(Into::into).collect())
}

fn clone_rune_string(s: &RuneString) -> Result<RuneString, RuntimeError> {
    let mut new_str = RuneString::new();
    new_str
        .try_push_str(s.as_str())
        .map_err(|e| RuntimeError::panic(e.to_string()))?;
    Ok(new_str)
}

fn generate_impl(this: &Spec) -> Result<Value, RuntimeError> {
    let mut rng = rand::rng();
    match this {
        Spec::Error(e) => Err(RuntimeError::panic(e.clone())),
        Spec::Just(v) => Ok(v.clone()),
        Spec::AlphaNumeric { len } => {
            let s: String = rng
                .sample_iter(rand::distr::Alphanumeric)
                .take(generate_impl(len.as_ref())?.as_usize()?)
                .map(char::from)
                .collect();
            rune::to_value(s)
        }
        Spec::String { len, min, max } => {
            let s: String = (0..generate_impl(len.as_ref())?.as_usize()?)
                .map(|_| rng.random_range(*min..*max))
                .collect();
            rune::to_value(s)
        }
        Spec::OneOf(values) => {
            let mut rng = rand::rng();
            values.choose(&mut rng).map_or_else(
                || Err(RuntimeError::panic("OneOf has no values")),
                generate_impl,
            )
        }
        Spec::Array { len, item } => rune::to_value(
            (0..generate_impl(len)?.as_usize()?)
                .map(|_| generate_impl(item))
                .collect::<Result<Vec<Value>, RuntimeError>>()?,
        ),
        Spec::Object(fields) => {
            // "hack" to get consistent ordering, because BTreeMap can't be used directly in Rune
            let mut obj = Object::with_capacity(fields.len())?;
            for (k, v) in fields {
                obj.insert(clone_rune_string(k)?, generate_impl(&v.into())?)?;
            }
            rune::to_value(obj)
        }
        Spec::Optional { p, item } => {
            let mut rng = rand::rng();
            rune::to_value(
                (rng.random::<f64>() < generate_impl(p)?.as_float()?)
                    .then(|| generate_impl(item))
                    .transpose()?,
            )
        }
        Spec::Tuple(values) => {
            let mut v = Vec::new();
            for spec in values {
                v.push(generate_impl(spec)?);
            }
            rune::to_value(v)
        }
        Spec::Bool => rune::to_value(rng.random::<bool>()),
        Spec::UInt { min, max } => rune::to_value(rng.random_range(*min..*max)),
        Spec::Int { min, max } => rune::to_value(rng.random_range(*min..*max)),
        Spec::Char { min, max } => rune::to_value(rng.random_range(*min..*max)),
        Spec::Float { min, max } => rune::to_value(rng.random_range(*min..*max)),
        Spec::Weighted(items) => {
            rune::to_value(items.choose_weighted(&mut rng, |v| v.0).map_or_else(
                |_| Err(RuntimeError::panic("OneOf has no values")),
                |(_, v)| generate_impl(v),
            )?)
        }
    }
}

#[rune::function]
fn spec(this: Value) -> Result<Value, String> {
    let spec: Spec = this.into();
    generate_impl(&spec).map_err(to_string)
}

#[rune::function(instance, path = to_spec)]
fn generate_object(this: Object) -> Result<Value, String> {
    generate_impl(&Spec::Object(this)).map_err(to_string)
}

#[rune::function(instance, path = to_spec)]
fn generate_vec(this: Vec<Value>) -> Result<Vec<Value>, String> {
    this.into_iter()
        .map(|v| generate_impl(&v.into()))
        .collect::<Result<Vec<Value>, RuntimeError>>()
        .map_err(to_string)
}

#[rune::function(instance, path = to_spec)]
fn generate(this: &Spec) -> Result<Value, String> {
    generate_impl(this).map_err(to_string)
}

pub fn spec_module() -> Result<Module, ContextError> {
    let mut m = Module::with_item(["spec"])?;
    m.ty::<Spec>()?;
    m.function_meta(range)?;
    m.function_meta(just)?;
    m.function_meta(literal)?;
    m.function_meta(bool)?;
    m.function_meta(char)?;
    m.function_meta(uint)?;
    m.function_meta(int)?;
    m.function_meta(float)?;
    m.function_meta(alphanumeric)?;
    m.function_meta(string)?;
    m.function_meta(one_of)?;
    m.function_meta(choose_one)?;
    m.function_meta(choose)?;
    m.function_meta(pick_one)?;
    m.function_meta(pick)?;
    m.function_meta(weighted)?;
    m.function_meta(array)?;
    m.function_meta(object)?;
    m.function_meta(optional)?;
    m.function_meta(tuple)?;
    m.function_meta(generate)?;
    m.function_meta(generate_object)?;
    m.function_meta(generate_vec)?;
    m.function_meta(spec)?;
    m.function_meta(values)?;
    Ok(m)
}
