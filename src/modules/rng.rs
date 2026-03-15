use rand::{
    RngExt,
    distr::{Distribution, StandardUniform},
    seq::IndexedRandom,
};
use rune::{ContextError, Module};

#[rune::function(instance)]
fn choose(values: &[rune::Value]) -> Option<rune::Value> {
    let mut rng = rand::rng();
    values.choose(&mut rng).cloned()
}

#[rune::function(instance)]
fn choose_many(values: &[rune::Value], count: usize) -> Vec<rune::Value> {
    let mut rng = rand::rng();
    values.sample(&mut rng, count).cloned().collect()
}

#[rune::function(instance)]
fn sample(values: &[rune::Value], count: usize) -> Vec<rune::Value> {
    let mut rng = rand::rng();
    values.sample(&mut rng, count).cloned().collect()
}

#[rune::function]
fn range(start: usize, end: usize) -> usize {
    let mut rng = rand::rng();
    rng.random_range(start..end)
}

#[rune::function]
fn range_many(start: usize, end: usize, count: usize) -> Vec<usize> {
    let mut rng = rand::rng();
    (0..count).map(|_| rng.random_range(start..end)).collect()
}

#[rune::function]
fn range_char(start: char, end: char) -> char {
    let mut rng = rand::rng();
    rng.random_range(start..end)
}

#[rune::function]
fn range_char_many(start: char, end: char, count: usize) -> Vec<char> {
    let mut rng = rand::rng();
    (0..count).map(|_| rng.random_range(start..end)).collect()
}

#[rune::function]
fn range_inclusive(start: usize, end: usize) -> usize {
    let mut rng = rand::rng();
    rng.random_range(start..=end)
}

#[rune::function]
fn range_inclusive_many(start: usize, end: usize, count: usize) -> Vec<usize> {
    let mut rng = rand::rng();
    (0..count).map(|_| rng.random_range(start..=end)).collect()
}

#[rune::function]
fn range_char_inclusive(start: char, end: char) -> char {
    let mut rng = rand::rng();
    rng.random_range(start..=end)
}

#[rune::function]
fn range_char_inclusive_many(start: char, end: char, count: usize) -> Vec<char> {
    let mut rng = rand::rng();
    (0..count).map(|_| rng.random_range(start..=end)).collect()
}

#[rune::function]
fn alpha_numeric() -> char {
    let mut rng = rand::rng();
    rng.sample(rand::distr::Alphanumeric) as char
}

#[rune::function]
fn alpha_numeric_many(count: usize) -> Vec<char> {
    let mut rng = rand::rng();
    (0..count)
        .map(|_| rng.sample(rand::distr::Alphanumeric) as char)
        .collect()
}

fn random<T>() -> T
where
    StandardUniform: Distribution<T>,
{
    let mut rng = rand::rng();
    rng.random()
}

fn random_many<T>(count: usize) -> Vec<T>
where
    StandardUniform: Distribution<T>,
{
    let mut rng = rand::rng();
    (0..count).map(|_| rng.random()).collect()
}

pub fn rng_module() -> Result<Module, ContextError> {
    let mut m = Module::with_item(["rng"])?;
    m.function_meta(choose)?;
    m.function_meta(choose_many)?;
    m.function_meta(sample)?;
    m.function_meta(range)?;
    m.function_meta(range_many)?;
    m.function_meta(range_char)?;
    m.function_meta(range_char_many)?;
    m.function_meta(range_inclusive)?;
    m.function_meta(range_inclusive_many)?;
    m.function_meta(range_char_inclusive)?;
    m.function_meta(range_char_inclusive_many)?;
    m.function_meta(alpha_numeric)?;
    m.function_meta(alpha_numeric_many)?;
    m.function("bool", random::<bool>).build()?;
    m.function("u8", random::<u8>).build()?;
    m.function("u16", random::<u16>).build()?;
    m.function("u32", random::<u32>).build()?;
    m.function("u64", random::<u64>).build()?;
    m.function("u128", random::<u128>).build()?;
    m.function("i8", random::<i8>).build()?;
    m.function("i16", random::<i16>).build()?;
    m.function("i32", random::<i32>).build()?;
    m.function("i64", random::<i64>).build()?;
    m.function("i128", random::<i128>).build()?;
    m.function("f32", random::<f32>).build()?;
    m.function("f64", random::<f64>).build()?;
    m.function("bool_many", random_many::<bool>).build()?;
    m.function("u8_many", random_many::<u8>).build()?;
    m.function("u16_many", random_many::<u16>).build()?;
    m.function("u32_many", random_many::<u32>).build()?;
    m.function("u64_many", random_many::<u64>).build()?;
    m.function("u128_many", random_many::<u128>).build()?;
    m.function("i8_many", random_many::<i8>).build()?;
    m.function("i16_many", random_many::<i16>).build()?;
    m.function("i32_many", random_many::<i32>).build()?;
    m.function("i64_many", random_many::<i64>).build()?;
    m.function("i128_many", random_many::<i128>).build()?;
    m.function("f32_many", random_many::<f32>).build()?;
    m.function("f64_many", random_many::<f64>).build()?;
    Ok(m)
}
