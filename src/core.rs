use anyhow::{anyhow, Result};
use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Value};
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, PartialEq, Clone)]
pub enum NodeKind {
    Number(f64),
    NumberArray(Vec<f64>),
    Formula(evalexpr::Node),
    SqlQuery(String),
}

#[derive(Debug, PartialEq, Clone)]
pub enum NodeOutput {
    NumberArray(Vec<f64>),
    Number(f64),
}

#[derive(Debug, PartialEq, Clone)]
pub struct EdgeDefinition {
    pub node_id: usize,
    pub input_id: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub struct NodeDefinition {
    pub node_id: usize,
    pub operation: String,
    pub kind: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Node {
    pub id: usize,
    pub inputs: RefCell<Vec<Rc<Self>>>,
    kind: NodeKind,
}

impl Node {
    pub fn from_float(value: f64) -> Result<Self> {
        Ok(Node {
            id: 0,
            inputs: RefCell::new(Vec::new()),
            kind: NodeKind::Number(value),
        })
    }

    pub fn from_float_vec(vec: &Vec<f64>) -> Result<Self> {
        Ok(Node {
            id: 0,
            inputs: RefCell::new(Vec::new()),
            kind: NodeKind::NumberArray(vec.to_vec()),
        })
    }

    pub fn from_formula(formula: &str) -> Result<Self> {
        let formula = build_operator_tree(&formula).unwrap();
        Ok(Node {
            id: 0,
            inputs: RefCell::new(Vec::new()),
            kind: NodeKind::Formula(formula),
        })
    }

    pub fn eval(&self) -> Result<NodeOutput> {
        let mut input_vals = Vec::new();
        let mut node_ids = Vec::new();
        let mut max_len = 0;
        let inputs = self.inputs.borrow_mut();
        for node in inputs.iter() {
            let val = node.eval()?;
            let val = match val {
                NodeOutput::Number(v) => vec![v],
                NodeOutput::NumberArray(v) => v,
            };
            max_len = max_len.max(val.len());
            node_ids.push(format!("id{}", node.id));
            input_vals.push(val);
        }

        match &self.kind {
            NodeKind::Number(_) => max_len = 1,
            NodeKind::NumberArray(_) => max_len = 1,
            _ => (),
        }

        let mut output_vals = Vec::new();
        for idx_arr in 0..max_len {
            match &self.kind {
                NodeKind::Number(val) => output_vals.push(val.clone()),
                NodeKind::NumberArray(vec) => output_vals.append(&mut vec.clone()),
                NodeKind::Formula(formula) => {
                    let mut args = HashMapContext::new();
                    for idx_node in 0..node_ids.len() {
                        let id = node_ids.get(idx_node).ok_or(anyhow!("indexing error"))?;

                        let node_vals = input_vals
                            .get(idx_node)
                            .ok_or(anyhow!("invalid node index"))?;

                        // Shorter arrays repeat the last value
                        let val = node_vals.get(idx_arr).unwrap_or(
                            node_vals
                                .last()
                                .expect("The value array from a node was empty"),
                        );

                        args.set_value(id.to_string(), Value::Float(*val))?;
                    }

                    let Ok(res) = formula.eval_float_with_context(&args) else {
                        return Err(anyhow!("Formula evaluation failed"));
                    };

                    output_vals.push(res);
                }
                NodeKind::SqlQuery(_q) => todo!(),
            }
        }

        match output_vals.len() {
            0 => Err(anyhow!("The computation resulted in no output")),
            1 => Ok(NodeOutput::Number(output_vals.first().unwrap().clone())),
            _ => Ok(NodeOutput::NumberArray(output_vals)),
        }
    }
}

pub struct Tree {
    nodes: HashMap<usize, Rc<Node>>,
}

impl Tree {
    pub fn new(nodes_definitions: Vec<NodeDefinition>, edge_definitions: Vec<EdgeDefinition>) -> Result<Self> {
        let mut nodes = HashMap::new();
        for node_def in &nodes_definitions {
            if let None = nodes.get_mut(&node_def.node_id) {
                let node = match node_def.kind {
                    0 => Rc::new(Node::from_formula(&node_def.operation)?),
                    _ => Err(anyhow!("Invalid node type"))?,
                };

                nodes.insert(node_def.node_id, Rc::clone(&node));
            }
        }


        for edge_def in &edge_definitions {
            let Some(node) = nodes.remove(&edge_def.node_id) else {
                return Err(anyhow!("node not found"));
            };

            let Some(input_node) = nodes.get(&edge_def.input_id) else {
                return Err(anyhow!("input node not found"));
            };

            {
                let mut inputs = node.inputs.borrow_mut();
                inputs.borrow_mut().push(Rc::clone(input_node));
            }

            nodes.insert(node.id, Rc::clone(&node));
        }

        let tree = Self { nodes };

        return Ok(tree);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_new() {
        let edge_defs = vec![
            EdgeDefinition{
                node_id: 2,
                input_id: 0,
            },
            EdgeDefinition{
                node_id: 2,
                input_id: 1,
            },
        ];
        let node_defs = vec![
            NodeDefinition{
                node_id: 0,
                kind: 0,
                operation: "a + 1".into(),
            },
            NodeDefinition{
                node_id: 1,
                kind: 0,
                operation: "b * 2".into(),
            },
            NodeDefinition{
                node_id: 2,
                kind: 0,
                operation: "id0 + id1".into(),
            },
        ];

        let tree = Tree::new(node_defs, edge_defs).unwrap();


    }

    #[test]
    fn test_formula() {
        let node1 = Node::from_float(1.5).unwrap();
        let mut node2 = Node::from_float_vec(&vec![5.5, 9.5]).unwrap();
        node2.id = 1;
        let mut node3 = Node::from_formula("(id1 - id0) / 2").unwrap();
        node3.id = 2;
        node3.inputs = RefCell::new(vec![Rc::new(node1), Rc::new(node2)]);

        let res = node3.eval().unwrap();
        assert_eq!(res, NodeOutput::NumberArray(vec![2., 4.]));

        let mut node4 = Node::from_formula("id2^2").unwrap();
        node4.id = 3;
        node4.inputs = RefCell::new(vec![Rc::new(node3)]);

        let res = node4.eval().unwrap();
        assert_eq!(res, NodeOutput::NumberArray(vec![4., 16.]));
    }
}
