//! Fields of a project file that the format does not know.
//!
//! Every struct of the format is `#[serde(default)]` and ignores unknown
//! fields, so a file from a newer editor still loads in an older game. A
//! typo (`AccelModifier((accel: ...))` instead of `accel_expr`) is ignored
//! the same way, which hides mistakes. [`from_ron_str_tracking`] parses with
//! [`serde_ignored`], which reports the path of every field the derived
//! deserializers skip; the project loaders turn them into warnings
//! ([`crate::format::ProjectV2::from_ron_str_with_report`]).

use serde::Deserialize;

/// One step of the path to an unknown field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldStep {
    /// A struct field or map key.
    Key(String),
    /// A list element.
    Index(usize),
}

/// Where an unknown field is: the steps to the struct that has it, and its
/// name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownField {
    /// Steps from the file's top level to the struct with the field.
    pub location: Vec<FieldStep>,
    /// The unknown field's name.
    pub name: String,
}

impl UnknownField {
    /// The location as text: `layers[0].effect.update_modifiers[2]` (empty
    /// at the top level).
    pub fn location_text(&self) -> String {
        let mut out = String::new();
        for step in self.location.iter() {
            match step {
                FieldStep::Key(key) => {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(key);
                }
                FieldStep::Index(i) => out.push_str(&format!("[{}]", i)),
            }
        }
        out
    }

    /// The index after the key `key` at the start of the location
    /// (`layers[3]` → 3).
    pub fn index_after(&self, key: &str) -> Option<usize> {
        match self.location.as_slice() {
            [FieldStep::Key(k), FieldStep::Index(i), ..] if k == key => Some(*i),
            _ => None,
        }
    }
}

fn steps(path: &serde_ignored::Path, out: &mut Vec<FieldStep>) {
    use serde_ignored::Path;
    match path {
        Path::Root => {}
        Path::Seq { parent, index } => {
            steps(parent, out);
            out.push(FieldStep::Index(*index));
        }
        Path::Map { parent, key } => {
            steps(parent, out);
            out.push(FieldStep::Key(key.clone()));
        }
        // Enum variants, `Some(..)` and newtype wrappers add no step.
        Path::Some { parent } | Path::NewtypeStruct { parent } | Path::NewtypeVariant { parent } => steps(parent, out),
    }
}

/// Parses RON like `ron::from_str` and lists the fields the deserializers
/// ignored.
pub fn from_ron_str_tracking<'a, T: Deserialize<'a>>(text: &'a str) -> ron::error::SpannedResult<(T, Vec<UnknownField>)> {
    let mut de = ron::Deserializer::from_str(text)?;
    let mut unknown = Vec::new();
    let value = serde_ignored::deserialize(&mut de, |path| {
        let mut location = Vec::new();
        steps(&path, &mut location);
        match location.pop() {
            Some(FieldStep::Key(name)) => unknown.push(UnknownField { location, name }),
            // An ignored list element or value without a key: report its
            // parent's last step as the name.
            Some(FieldStep::Index(i)) => unknown.push(UnknownField { location, name: format!("[{}]", i) }),
            None => {}
        }
    });
    let value = value.map_err(|e| de.span_error(e))?;
    de.end().map_err(|e| de.span_error(e))?;
    Ok((value, unknown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Deserialize, Serialize, Default)]
    #[serde(default)]
    struct Inner {
        a: f32,
    }

    #[derive(Deserialize, Serialize, Default)]
    #[serde(default)]
    struct Outer {
        list: Vec<Kind>,
        inner: Option<Inner>,
    }

    #[derive(Deserialize, Serialize)]
    enum Kind {
        One(Inner),
    }

    #[test]
    fn paths_name_list_elements_and_skip_variants() {
        let (_, unknown): (Outer, _) =
            from_ron_str_tracking("(list: [One((a: 1.0)), One((b: 2.0))], inner: Some((c: 3.0)), typo: 1)").unwrap();
        let texts: Vec<(String, String)> = unknown.iter().map(|u| (u.location_text(), u.name.clone())).collect();
        assert_eq!(
            texts,
            vec![
                ("list[1]".to_string(), "b".to_string()),
                ("inner".to_string(), "c".to_string()),
                (String::new(), "typo".to_string()),
            ]
        );
        assert_eq!(unknown[0].index_after("list"), Some(1));
        let (_, none): (Outer, _) = from_ron_str_tracking("(list: [One((a: 1.0))])").unwrap();
        assert!(none.is_empty());
        // Errors keep their position.
        let err = from_ron_str_tracking::<Outer>("(list: [One((a: x))])").err().unwrap();
        assert!(err.position.col > 1, "{}", err);
    }
}
