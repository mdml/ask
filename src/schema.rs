//! Structural checks whose diagnostics contain only static schema labels.

use toml::{Table, Value};

type Field = (&'static str, fn(&Value) -> bool, bool);

const ROOT: &[Field] = &[
    ("default_profile", Value::is_str, true),
    ("providers", Value::is_table, true),
    ("profiles", Value::is_table, true),
];
const PROVIDER: &[Field] = &[
    ("kind", Value::is_str, true),
    ("base_url", Value::is_str, true),
    ("api_key_env", Value::is_str, true),
    ("timeout_ms", unsigned, false),
];
const PROFILE: &[Field] = &[
    ("provider", Value::is_str, true),
    ("model", Value::is_str, true),
    ("system_prompt", Value::is_str, false),
];

pub(super) fn check(value: &Value) -> Result<(), String> {
    let root = table(value, "configuration")?;
    fields(root, "configuration", ROOT)?;
    entries(&root["providers"], "providers", PROVIDER)?;
    entries(&root["profiles"], "profiles", PROFILE)
}

fn table<'a>(value: &'a Value, path: &str) -> Result<&'a Table, String> {
    value
        .as_table()
        .ok_or_else(|| format!("{path}: expected a table"))
}

fn entries(value: &Value, path: &str, schema: &[Field]) -> Result<(), String> {
    for (index, value) in table(value, path)?.values().enumerate() {
        let path = format!("{path}[{}]", index + 1);
        fields(table(value, &path)?, &path, schema)?;
    }
    Ok(())
}

fn fields(values: &Table, path: &str, schema: &[Field]) -> Result<(), String> {
    known_fields(values, path, schema)?;
    for (name, accepts, required) in schema {
        match values.get(*name) {
            None if *required => return Err(format!("{path}.{name}: missing required field")),
            Some(value) if !accepts(value) => {
                return Err(format!("{path}.{name}: invalid type or range"));
            }
            _ => (),
        }
    }
    Ok(())
}

fn known_fields(values: &Table, path: &str, schema: &[Field]) -> Result<(), String> {
    for (index, key) in values.keys().enumerate() {
        if !schema.iter().any(|(name, _, _)| key == name) {
            return Err(format!("{path}: unknown field at key index {}", index + 1));
        }
    }
    Ok(())
}

fn unsigned(value: &Value) -> bool {
    value.as_integer().is_some_and(|number| number >= 0)
}
