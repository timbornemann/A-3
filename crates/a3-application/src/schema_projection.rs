//! Projection of build-owned schemas, never model or repository input.
use serde_json::Value;
use std::collections::BTreeSet;

/// Removes definitions unreachable from the selected root. Cycles terminate at the fixed point.
pub(crate) fn prune_definitions(schema: &mut Value) -> Option<()> {
    let mut used = BTreeSet::new();
    references(schema, &mut used);
    loop {
        let before = used.clone();
        for name in &before {
            references(schema.get("$defs")?.get(name)?, &mut used);
        }
        if before == used {
            break;
        }
    }
    schema
        .get_mut("$defs")?
        .as_object_mut()?
        .retain(|name, _| used.contains(name));
    Some(())
}

fn references(value: &Value, used: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|s| s.strip_prefix("#/$defs/"))
            {
                used.insert(reference.to_owned());
            }
            for (key, child) in object {
                if key != "$defs" {
                    references(child, used);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                references(child, used);
            }
        }
        _ => {}
    }
}
