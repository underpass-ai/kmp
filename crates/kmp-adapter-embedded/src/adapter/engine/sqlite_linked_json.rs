use kmp_domain::PortError;
use rusqlite::{Connection, types::Value};

use super::{KeyShape, LinkedJsonRow, LinkedJsonScan, scan_shape_mismatch};

pub(super) fn scan(
    connection: &Connection,
    request: &LinkedJsonScan<'_>,
) -> Result<Vec<LinkedJsonRow>, PortError> {
    let (sql, parameters) = query(request)?;
    let mut statement = connection.prepare_cached(&sql).map_err(error)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(parameters), |row| {
            Ok(LinkedJsonRow {
                source: row.get(0)?,
                target: row.get(1)?,
                source_json: row.get::<_, Option<String>>(2)?.map(String::into_bytes),
                target_json: row.get::<_, Option<String>>(3)?.map(String::into_bytes),
            })
        })
        .map_err(error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(error)
}

pub(super) fn query(request: &LinkedJsonScan<'_>) -> Result<(String, Vec<Value>), PortError> {
    for (table, shape) in [
        (request.roots, KeyShape::Str),
        (request.links, KeyShape::Str3),
        (request.records, KeyShape::Str),
    ] {
        if table.key_shape() != shape {
            return Err(scan_shape_mismatch(table, shape));
        }
    }
    let mut parameters = vec![Value::Text(request.relation.into())];
    let source = projection("source", request.fields, &mut parameters);
    let target = projection("target", request.fields, &mut parameters);
    let cursor = match request.after {
        Some((root, target)) => {
            parameters.push(Value::Text(root.into()));
            let root_parameter = parameters.len();
            parameters.push(Value::Text(target.into()));
            // A tuple OR alone cannot seek k2 inside a high-degree root.
            // Supply its correlated lower bound too. Empty is the minimum
            // non-null text key, including an empty target on a later root.
            format!(
                "AND roots.k >= ?{root_parameter} \
                AND links.k2 >= CASE WHEN roots.k = ?{root_parameter} THEN ?{} ELSE '' END \
                AND (roots.k > ?{root_parameter} OR links.k2 > ?{})",
                parameters.len(),
                parameters.len()
            )
        }
        None => String::new(),
    };
    parameters.push(Value::Integer(i64::from(request.limit)));
    let sql = format!(
        "SELECT roots.k, links.k2, {source}, {target} \
         FROM \"{}\" AS roots \
         CROSS JOIN \"{}\" AS links ON links.k1 = roots.k AND links.k3 = ?1 \
         LEFT JOIN \"{}\" AS source ON source.k = roots.k \
         LEFT JOIN \"{}\" AS target ON target.k = links.k2 \
         WHERE 1 {cursor} ORDER BY roots.k, links.k2 LIMIT ?{}",
        request.roots,
        request.links,
        request.records,
        request.records,
        parameters.len(),
    );
    Ok((sql, parameters))
}

fn projection(alias: &str, fields: &[&str], parameters: &mut Vec<Value>) -> String {
    let mut pairs = vec![];
    for field in fields {
        let path = field
            .split('.')
            .map(|part| serde_json::to_string(part).expect("string JSON"))
            .fold(String::from("$"), |path, part| format!("{path}.{part}"));
        parameters.push(Value::Text((*field).into()));
        let name = parameters.len();
        parameters.push(Value::Text(path));
        pairs.push(format!(
            "?{name}, json_extract({alias}.v, ?{})",
            parameters.len()
        ));
    }
    format!(
        "CASE WHEN {alias}.k IS NULL THEN NULL ELSE json_object({}) END",
        pairs.join(", ")
    )
}

fn error(error: rusqlite::Error) -> PortError {
    PortError::Unavailable(format!("embedded linked JSON read failed: {error}"))
}
