use anyhow::{anyhow, Result};
use futures::executor;
use sqlx::Row;
use sqlx::{Connection, SqliteConnection};
use std::borrow::BorrowMut;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::node::Node;

pub struct Tree {
    nodes: HashMap<i32, Rc<Node>>,
    root: Rc<Node>,
}

impl Tree {
    pub fn new(root_node_id: i32) -> Result<Self> {
        let mut conn = executor::block_on(SqliteConnection::connect("test_db.db"))?;

        let edges = executor::block_on(
            sqlx::query("
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
            .bind(root_node_id)
            .fetch_all(&mut conn),
        )?;

        let mut node_ids = HashSet::new();
        for edge in &edges {
            let node_id: i32 = edge.try_get("node_id")?;
            let input_id: i32 = edge.try_get("input_id")?;
            node_ids.insert(node_id);
            node_ids.insert(input_id);
        }

        let placeholders = "?,".repeat(node_ids.len()).trim_matches(',').to_string() + ")";

        let query = "
            SELECT * FROM node 
            WHERE node_id IN ( 
        ".to_string() + &placeholders;
        let mut query = sqlx::query(&query);

        for node_id in node_ids {
            query = query.bind(node_id);
        }

        let nodes_definitions = executor::block_on(
            query.fetch_all(&mut conn),
        )?;

        let mut nodes = HashMap::new();
        let mut root = None;
        for row in &nodes_definitions {
            let node_id: i32 = row.try_get("node_id")?;
            let node_type: i32 = row.try_get("type")?;
            let operation: String = row.try_get("operation")?;

            if let None = nodes.get_mut(&node_id) {
                let node = match node_type {
                    0 => Rc::new(Node::from_formula(&operation)?),
                    _ => Err(anyhow!("Invalid node type"))?,
                };

                if node_id == root_node_id {
                    root = Some(Rc::clone(&node));
                }

                nodes.insert(node_id, Rc::clone(&node));
            }
        }


        for row in &edges {
            let node_id: i32 = row.try_get("node_id")?;
            let input_id: i32 = row.try_get("input_id")?;

            let Some(node) = nodes.remove(&node_id) else {
                // TODO(marco)
                dbg!(&nodes);
                dbg!(&input_id);
                return Err(anyhow!("node not found"));
            };

            let Some(input_node) = nodes.get(&input_id) else {
                return Err(anyhow!("input node not found"));
            };

            {
                let mut inputs = node.inputs.borrow_mut();
                inputs.borrow_mut().push(Rc::clone(input_node));
            }

            nodes.insert(node.id, Rc::clone(&node));
        }

        let Some(root) = root else {
            return Err(anyhow!("Root node not found"));
        };

        let tree = Self { nodes, root };

        return Ok(tree);
    }
}

/*
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

INSERT INTO "main"."node"("node_id","type","computation","name","symbol") VALUES (1,0,'a + 2',NULL,NULL);

WITH RECURSIVE child_tree AS (
    SELECT node_id, input_id
    FROM edge
    WHERE node_id = 3

    UNION ALL

    SELECT c.node_id, c.input_id
    FROM edge c
    JOIN child_tree ct ON c.node_id = ct.input_id
)

SELECT * FROM child_tree;
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_from_sqlite() {
        let tree = Tree::new(3).unwrap();

        //tree.eval()

        //assert_eq!(root, node);
    }
}
