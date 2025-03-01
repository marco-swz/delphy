use anyhow::Result;
use futures::executor;
use sqlx::Row;
use sqlx::{Connection, SqliteConnection};
use std::collections::HashSet;

use crate::core::{EdgeDefinition, NodeDefinition};

pub fn defintions_from_sqlite(
    file_name: String,
    root_node_id: usize,
) -> Result<(Vec<NodeDefinition>, Vec<EdgeDefinition>)> {
    let mut conn = executor::block_on(SqliteConnection::connect(&file_name))?;
    let edge_query = executor::block_on(
        sqlx::query(
            "
            WITH RECURSIVE child_tree AS (
                SELECT node_id, input_id
                FROM edge
                WHERE node_id = ?

                UNION ALL

                SELECT c.node_id, c.input_id
                FROM edge c
                JOIN child_tree ct ON c.node_id = ct.input_id
            )

            SELECT node_id, input_id FROM child_tree
            ",
        )
        .bind(root_node_id as u32)
        .fetch_all(&mut conn),
    )?;

    let mut node_ids = HashSet::new();
    let mut edge_definitions = Vec::new();
    for edge in &edge_query {
        let node_id: i32 = edge.try_get("node_id")?;
        let input_id: i32 = edge.try_get("input_id")?;
        node_ids.insert(node_id);
        node_ids.insert(input_id);
        edge_definitions.push(EdgeDefinition {
            node_id: node_id as usize,
            input_id: node_id as usize,
        });
    }

    let placeholders = "?,".repeat(node_ids.len()).trim_matches(',').to_string() + ")";

    let query = "
        SELECT * FROM node 
        WHERE node_id IN ( 
    "
    .to_string()
        + &placeholders;
    let mut query = sqlx::query(&query);

    for node_id in node_ids {
        query = query.bind(node_id);
    }

    let node_query = executor::block_on(query.fetch_all(&mut conn))?;

    let mut nodes_definitions = Vec::new();
    for row in &node_query {
        let node_id: i32 = row.try_get("node_id")?;
        let kind: i32 = row.try_get("type")?;
        let node_def = NodeDefinition {
            node_id: node_id as usize,
            kind: kind as usize,
            value: row.try_get("operation")?,
        };
        nodes_definitions.push(node_def);
    }

    return Ok((nodes_definitions, edge_definitions));
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor;
    use sqlx::sqlite::SqliteConnectOptions;

    #[test]
    fn test_definitions_from_sqlite() {
        let options = SqliteConnectOptions::new()
            .filename("_test_db.db")
            .create_if_missing(true);

        let mut conn = executor::block_on(SqliteConnection::connect_with(&options)).unwrap();

        executor::block_on(sqlx::query(r#"
            CREATE TABLE "node" (
                "node_id"	INTEGER NOT NULL UNIQUE,
                "type"	INTEGER NOT NULL,
                "operation"	BLOB NOT NULL,
                "name"	TEXT,
                "symbol"	TEXT,
                PRIMARY KEY("node_id" AUTOINCREMENT)
            );

            CREATE TABLE "edge" (
                "edge_id"	INTEGER NOT NULL UNIQUE,
                "node_id"	INTEGER NOT NULL,
                "input_id"	INTEGER NOT NULL,
                PRIMARY KEY("edge_id" AUTOINCREMENT),
                FOREIGN KEY("input_id") REFERENCES "edge"("edge_id"),
                FOREIGN KEY("node_id") REFERENCES "node"("node_id")
            );

            INSERT INTO "main"."node"("node_id","type","operation","name","symbol") VALUES (1,0,'a + 2',NULL,NULL);
            INSERT INTO "main"."node"("node_id","type","operation","name","symbol") VALUES (2,1,'a * 2',NULL,NULL);
            INSERT INTO "main"."node"("node_id","type","operation","name","symbol") VALUES (3,2,'id0 + id1',NULL,NULL);
        "#).execute(&mut conn)).unwrap();

        let (node_defs, edge_defs) = defintions_from_sqlite("_test_db.db".into(), 3).unwrap();

        assert_eq!(node_defs.len(), 3);
        for def in node_defs {
            match def.node_id {
                1 => {
                    assert_eq!(def.kind, 0);
                    assert_eq!(def.value, "a + 2");
                }
                2 => {
                    assert_eq!(def.kind, 1);
                    assert_eq!(def.value, "a * 2");
                }
                3 => {
                    assert_eq!(def.kind, 2);
                    assert_eq!(def.value, "id0 + id1");
                }
                _ => assert!(false),
            };
        }
    }
}
