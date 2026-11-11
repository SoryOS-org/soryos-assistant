use std::collections::BTreeSet;

use serde_json::{Map, Value};

pub(crate) fn merge_json(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match target_map.get_mut(key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        target_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target_value, patch_value) => {
            *target_value = patch_value.clone();
        }
    }
}

pub(crate) fn diff_values(before: &Value, after: &Value) -> Value {
    if before == after {
        return Value::Null;
    }

    match (before, after) {
        (Value::Object(before_map), Value::Object(after_map)) => {
            let mut keys = BTreeSet::new();
            keys.extend(before_map.keys().cloned());
            keys.extend(after_map.keys().cloned());

            let mut changes = Map::new();
            for key in keys {
                let before_value = before_map.get(&key);
                let after_value = after_map.get(&key);

                match (before_value, after_value) {
                    (Some(b), Some(a)) => {
                        let child_diff = diff_values(b, a);
                        if !child_diff.is_null() {
                            changes.insert(key, child_diff);
                        }
                    }
                    (None, Some(a)) => {
                        let mut detail = Map::new();
                        detail.insert("before".into(), Value::Null);
                        detail.insert("after".into(), a.clone());
                        changes.insert(key, Value::Object(detail));
                    }
                    (Some(b), None) => {
                        let mut detail = Map::new();
                        detail.insert("before".into(), b.clone());
                        detail.insert("after".into(), Value::Null);
                        changes.insert(key, Value::Object(detail));
                    }
                    (None, None) => {}
                }
            }

            if changes.is_empty() {
                Value::Null
            } else {
                Value::Object(changes)
            }
        }
        _ => {
            let mut detail = Map::new();
            detail.insert("before".into(), before.clone());
            detail.insert("after".into(), after.clone());
            Value::Object(detail)
        }
    }
}
