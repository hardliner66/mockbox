use std::collections::HashMap;

use rand::{RngExt, seq::IndexedRandom};
use rune::{Any, ContextError, Module, Value, runtime::RuntimeError};

use crate::helper::to_string;

#[derive(Any, Debug)]
enum Spec {
    Just(Value),

    Bool,

    UInt {
        min: u128,
        max: u128,
    },
    Int {
        min: i128,
        max: i128,
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
    Object(HashMap<String, Spec>),
    Optional {
        p: Box<Spec>,
        item: Box<Spec>,
    },
    Tuple(Vec<Spec>),
}

#[rune::function]
fn just(value: Value) -> Spec {
    Spec::Just(value)
}

#[rune::function]
fn bool() -> Spec {
    Spec::Bool
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

#[rune::function]
fn alphanumeric(len: Spec) -> Spec {
    Spec::AlphaNumeric { len: Box::new(len) }
}

#[rune::function]
fn string(len: Spec, min: char, max: char) -> Spec {
    Spec::String {
        len: Box::new(len),
        min,
        max,
    }
}

#[rune::function]
fn one_of(values: Vec<Spec>) -> Spec {
    Spec::OneOf(values)
}

#[rune::function]
fn weighted(values: Vec<(u32, Spec)>) -> Spec {
    Spec::Weighted(values)
}

#[rune::function]
fn array(len: Spec, item: Spec) -> Spec {
    Spec::Array {
        len: Box::new(len),
        item: Box::new(item),
    }
}

#[rune::function]
fn object(fields: HashMap<String, Spec>) -> Spec {
    Spec::Object(fields)
}

#[rune::function]
fn optional(p: Spec, item: Spec) -> Spec {
    Spec::Optional {
        p: Box::new(p),
        item: Box::new(item),
    }
}

#[rune::function]
fn tuple(items: Vec<Spec>) -> Spec {
    Spec::Tuple(items)
}

fn generate_impl(this: &Spec) -> Result<Value, RuntimeError> {
    let mut rng = rand::rng();
    match this {
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
        Spec::Object(fields) => rune::to_value(
            fields
                .iter()
                .map(|(k, spec)| Ok((k.to_owned(), generate_impl(spec)?)))
                .collect::<Result<HashMap<String, Value>, RuntimeError>>()
                .map_err(|e| RuntimeError::panic(e.to_string()))?,
        ),
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
        Spec::Float { min, max } => rune::to_value(rng.random_range(*min..*max)),
        Spec::Weighted(items) => {
            rune::to_value(items.choose_weighted(&mut rng, |v| v.0).map_or_else(
                |_| Err(RuntimeError::panic("OneOf has no values")),
                |(_, v)| generate_impl(v),
            )?)
        }
    }
}

#[rune::function(instance)]
fn generate(this: &Spec) -> Result<Value, String> {
    generate_impl(this).map_err(to_string)
}

pub fn spec_module() -> Result<Module, ContextError> {
    let mut m = Module::with_item(["spec"])?;
    m.ty::<Spec>()?;
    m.function_meta(just)?;
    m.function_meta(bool)?;
    m.function_meta(uint)?;
    m.function_meta(int)?;
    m.function_meta(float)?;
    m.function_meta(alphanumeric)?;
    m.function_meta(string)?;
    m.function_meta(one_of)?;
    m.function_meta(weighted)?;
    m.function_meta(array)?;
    m.function_meta(object)?;
    m.function_meta(optional)?;
    m.function_meta(tuple)?;
    m.function_meta(generate)?;
    Ok(m)
}
