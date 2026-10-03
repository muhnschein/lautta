// SPDX-License-Identifier: LGPL-2.1-or-later
//! Structured values cross the QML boundary as JSON text
//! (doc/QML-API.md); plain values become QVariants.

use qmetaobject::{QString, QVariant};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Serialises `value`; serialisation of our own plain data cannot fail, so
/// the fallback (`null`) is never seen in practice.
pub fn to_json<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

pub fn from_json<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

/// Plain JSON values map to QVariants QML reads natively; arrays and
/// objects stay JSON text.
pub fn json_value_to_qvariant(value: &serde_json::Value) -> QVariant {
    use serde_json::Value;
    match value {
        Value::Null => QVariant::default(),
        Value::Bool(b) => QVariant::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => QVariant::from(i),
            None => QVariant::from(n.as_f64().unwrap_or_default()),
        },
        Value::String(s) => QVariant::from(QString::from(s.as_str())),
        other => QVariant::from(QString::from(to_json(other).as_str())),
    }
}

pub fn qs(s: &str) -> QString {
    QString::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let v = vec!["lautta://a/b".to_owned()];
        let text = to_json(&v);
        assert_eq!(text, r#"["lautta://a/b"]"#);
        assert_eq!(from_json::<Vec<String>>(&text), Some(v));
        assert_eq!(from_json::<Vec<String>>("not json"), None);
    }

    #[test]
    fn plain_values_become_variants() {
        assert!(json_value_to_qvariant(&serde_json::json!(true)).to_bool());
        assert_eq!(json_value_to_qvariant(&serde_json::json!(7)).to_int(), 7);
        assert_eq!(
            json_value_to_qvariant(&serde_json::json!("x"))
                .to_qstring()
                .to_string(),
            "x"
        );
        assert_eq!(
            json_value_to_qvariant(&serde_json::json!(["a"]))
                .to_qstring()
                .to_string(),
            r#"["a"]"#
        );
        assert!(!json_value_to_qvariant(&serde_json::Value::Null).is_valid());
    }
}
