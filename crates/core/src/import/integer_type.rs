pub(super) fn is_integer_type_name(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "integer"
            | "tinyint"
            | "smallint"
            | "mediumint"
            | "bigint"
            | "int2"
            | "int4"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "serial"
            | "serial2"
            | "serial4"
            | "serial8"
            | "smallserial"
            | "bigserial"
    )
}
