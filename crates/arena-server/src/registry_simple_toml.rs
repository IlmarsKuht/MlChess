use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};

#[derive(Debug, Default)]
pub(crate) struct SimpleToml {
    root: BTreeMap<String, SimpleTomlValue>,
    sections: BTreeMap<String, BTreeMap<String, SimpleTomlValue>>,
}

#[derive(Debug, Clone)]
enum SimpleTomlValue {
    String(String),
    Bool(bool),
    Integer(i64),
    StringArray(Vec<String>),
}

impl SimpleToml {
    pub(crate) fn require_string(&self, key: &str) -> Result<String> {
        match self.root.get(key) {
            Some(SimpleTomlValue::String(value)) => Ok(value.clone()),
            Some(_) => bail!("expected {key} to be a string"),
            None => bail!("missing required key {key}"),
        }
    }

    pub(crate) fn optional_string(&self, key: &str) -> Result<Option<String>> {
        match self.root.get(key) {
            Some(SimpleTomlValue::String(value)) => Ok(Some(value.clone())),
            Some(_) => bail!("expected {key} to be a string"),
            None => Ok(None),
        }
    }

    pub(crate) fn optional_bool(&self, key: &str) -> Result<Option<bool>> {
        match self.root.get(key) {
            Some(SimpleTomlValue::Bool(value)) => Ok(Some(*value)),
            Some(_) => bail!("expected {key} to be a bool"),
            None => Ok(None),
        }
    }

    pub(crate) fn optional_integer(&self, key: &str) -> Result<Option<i64>> {
        match self.root.get(key) {
            Some(SimpleTomlValue::Integer(value)) => Ok(Some(*value)),
            Some(_) => bail!("expected {key} to be an integer"),
            None => Ok(None),
        }
    }

    pub(crate) fn require_bool(&self, key: &str) -> Result<bool> {
        self.optional_bool(key)?
            .ok_or_else(|| anyhow!("missing required key {key}"))
    }

    pub(crate) fn require_integer(&self, key: &str) -> Result<i64> {
        self.optional_integer(key)?
            .ok_or_else(|| anyhow!("missing required key {key}"))
    }

    pub(crate) fn optional_string_array(&self, key: &str) -> Result<Vec<String>> {
        match self.root.get(key) {
            Some(SimpleTomlValue::StringArray(value)) => Ok(value.clone()),
            Some(_) => bail!("expected {key} to be an array of strings"),
            None => Ok(Vec::new()),
        }
    }

    pub(crate) fn string_map(&self, section: &str) -> Result<BTreeMap<String, String>> {
        let mut values = BTreeMap::new();
        let Some(section_values) = self.sections.get(section) else {
            return Ok(values);
        };

        for (key, value) in section_values {
            match value {
                SimpleTomlValue::String(value) => {
                    values.insert(key.clone(), value.clone());
                }
                _ => bail!("expected [{section}].{key} to be a string"),
            }
        }

        Ok(values)
    }
}

pub(crate) fn parse_simple_toml(text: &str) -> Result<SimpleToml> {
    let mut document = SimpleToml::default();
    let mut current_section: Option<String> = None;

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                bail!("unterminated section header on line {}", line_idx + 1);
            }
            current_section = Some(line[1..line.len() - 1].trim().to_string());
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid assignment on line {}", line_idx + 1))?;
        let key = key.trim().to_string();
        let value = parse_simple_toml_value(value.trim())
            .with_context(|| format!("invalid value for {key} on line {}", line_idx + 1))?;

        if let Some(section) = &current_section {
            document
                .sections
                .entry(section.clone())
                .or_default()
                .insert(key, value);
        } else {
            document.root.insert(key, value);
        }
    }

    Ok(document)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in line.char_indices() {
        match ch {
            '"' if !escaped => {
                in_string = !in_string;
                escaped = false;
            }
            '#' if !in_string => return &line[..idx],
            '\\' if in_string => {
                escaped = !escaped;
            }
            _ => {
                escaped = false;
            }
        }
    }

    line
}

fn parse_simple_toml_value(value: &str) -> Result<SimpleTomlValue> {
    if value.starts_with('"') {
        return Ok(SimpleTomlValue::String(
            serde_json::from_str(value).context("invalid string literal")?,
        ));
    }

    if value.starts_with('[') {
        let values: Vec<String> =
            serde_json::from_str(value).context("invalid string array literal")?;
        return Ok(SimpleTomlValue::StringArray(values));
    }

    if value == "true" || value == "false" {
        return Ok(SimpleTomlValue::Bool(value.parse()?));
    }

    Ok(SimpleTomlValue::Integer(
        value.parse().context("invalid integer literal")?,
    ))
}
