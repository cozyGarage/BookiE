use apache_avro::Reader;
use apache_avro::types::Value;

pub const MAX_RECORDS: usize = 1000;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum AvroError {
    TooLarge,
    Invalid(String),
}

pub fn decode_container(bytes: &[u8]) -> Result<(Vec<serde_json::Value>, bool), AvroError> {
    if bytes.len() > MAX_BYTES {
        return Err(AvroError::TooLarge);
    }
    let reader = Reader::new(bytes).map_err(|error| AvroError::Invalid(error.to_string()))?;
    let mut records = Vec::new();
    for record in reader {
        let value = record.map_err(|error| AvroError::Invalid(error.to_string()))?;
        if records.len() == MAX_RECORDS {
            return Ok((records, true));
        }
        records.push(to_json(value));
    }
    Ok((records, false))
}

fn to_json(value: Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(b),
        Value::Int(n) => serde_json::Value::from(n),
        Value::Long(n) => serde_json::Value::from(n),
        Value::Float(f) => serde_json::Value::from(f),
        Value::Double(f) => serde_json::Value::from(f),
        Value::String(s) => serde_json::Value::String(s),
        Value::Enum(_, symbol) => serde_json::Value::String(symbol),
        Value::Union(_, inner) => to_json(*inner),
        Value::Array(items) => serde_json::Value::Array(items.into_iter().map(to_json).collect()),
        Value::Map(entries) => serde_json::Value::Object(entries.into_iter().map(|(k, v)| (k, to_json(v))).collect()),
        Value::Record(fields) => serde_json::Value::Object(fields.into_iter().map(|(k, v)| (k, to_json(v))).collect()),
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apache_avro::{Schema, Writer};

    fn container(records: usize) -> Vec<u8> {
        let schema = Schema::parse_str(
            r#"{"type":"record","name":"Order","fields":[{"name":"id","type":"long"},{"name":"note","type":["null","string"]}]}"#,
        )
        .unwrap();
        let mut writer = Writer::new(&schema, Vec::new());
        for n in 0..records {
            let mut record = apache_avro::types::Record::new(&schema).unwrap();
            record.put("id", n as i64);
            record.put(
                "note",
                if n % 2 == 0 {
                    Value::Union(1, Box::new(Value::String(format!("n{n}"))))
                } else {
                    Value::Union(0, Box::new(Value::Null))
                },
            );
            writer.append(record).unwrap();
        }
        writer.into_inner().unwrap()
    }

    #[test]
    fn a_container_decodes_with_its_own_schema_and_unions_unwrap() {
        let (records, more) = decode_container(&container(3)).unwrap();
        assert!(!more);
        assert_eq!(records[0], serde_json::json!({"id": 0, "note": "n0"}));
        assert_eq!(records[1], serde_json::json!({"id": 1, "note": null}));
    }

    #[test]
    fn a_container_over_the_record_limit_says_so() {
        let (records, more) = decode_container(&container(MAX_RECORDS + 1)).unwrap();
        assert_eq!(records.len(), MAX_RECORDS);
        assert!(more);
    }

    #[test]
    fn bytes_that_are_not_a_container_are_refused_not_guessed() {
        assert!(matches!(decode_container(b"not avro"), Err(AvroError::Invalid(_))));
        let too_big = vec![0u8; MAX_BYTES + 1];
        assert_eq!(decode_container(&too_big), Err(AvroError::TooLarge));
    }
}
