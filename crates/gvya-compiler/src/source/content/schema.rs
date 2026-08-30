//! Schema source decoding.

use super::super::*;

pub(in crate::source) fn parse_named_type(
    value: &JsonValue,
    path: &str,
    limits: SourceLimits,
    issues: &mut Vec<SourceIssue>,
) -> Option<NamedTypeDefinition> {
    let obj = expect_object(value, path, issues).ok()?;
    reject_unknown_keys(obj, crate::source::contract::NAMED_TYPE_KEYS, path, issues);
    let schema_value = required_value(obj, "schema", path, issues)?;
    Some(NamedTypeDefinition {
        id: TypeId::new(required_string(obj, "id", path, limits, issues)?),
        schema: compile_schema_source(schema_value, &format!("{path}.schema"), issues)?,
    })
}

pub(in crate::source) fn compile_schema_source(
    value: &JsonValue,
    path: &str,
    issues: &mut Vec<SourceIssue>,
) -> Option<gvya_kernel::capability::ValueSchema> {
    match compile_json_schema(value, SchemaLimits::default()) {
        Ok(schema) => Some(schema),
        Err(rows) => {
            for row in rows {
                issues.push(SourceIssue {
                    code: format!("source.{}", row.code),
                    path: Some(format!("{path}:{}", row.path)),
                    message: row.message,
                });
            }
            None
        }
    }
}
