use std::fmt::Display;

pub fn to_string<T: Display>(value: T) -> String {
    value.to_string()
}
