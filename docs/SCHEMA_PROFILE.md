# GVYA Executable JSON Schema Profile v1

Capability contracts are authored as JSON Schema 2020-12 documents. GVYA does **not** pretend to execute every JSON Schema keyword. compiler/artifact layer compiles a deliberately bounded assertion profile into the capability-kernel `ValueSchema` runtime IR.

Unsupported assertion keywords are compiler errors. They are never ignored.

## Supported executable keywords

- `type` — one string or a bounded array of types
- `enum`
- `oneOf`
- `minimum`, `maximum`
- `minLength`, `maxLength` — Unicode character counts
- `items` — one schema
- `minItems`, `maxItems`
- `properties`
- `required`
- `additionalProperties` — boolean only and **explicitly required for object schemas**
- `minProperties`, `maxProperties`

Annotation/identity keywords accepted but not used as executable authority:

- `$schema`
- `$id`
- `title`
- `description`

Examples of intentionally rejected v1 keywords include `$ref`, `pattern`, `format`, `allOf`, `anyOf`, `not`, `if/then/else`, `dependentSchemas`, `contains`, tuple-style `prefixItems`, `patternProperties`, `unevaluatedProperties`, exclusive numeric bounds, and arbitrary extensions. They may be added later only when compiler + runtime can enforce their exact semantics.

## String limits

`minLength`/`maxLength` are compiled as Unicode character constraints. Separately, `SchemaLimits.max_string_bytes` is an absolute runtime safety ceiling. The two concerns are not conflated.

## Objects

JSON Schema normally defaults `additionalProperties` to `true`. GVYA does not silently change that default. Instead, the executable profile requires the author/compiler source to say `additionalProperties: true` or `false` explicitly. This makes capability argument openness visible in review and Why/audit surfaces.

## `enum` combinations

For scalar boolean/number/integer schemas, enum members are checked against the declared type/bounds before compilation. String enums retain string length constraints and allowed values together.

Object/array schemas combined with `enum` are rejected in v1 rather than weakening one side of the conjunction. Authors can use `oneOf` when the runtime-supported semantics are expressible.
