use anyhow::{anyhow, Result};
use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Value};
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type NodeId = usize;
type EvalId = u64;

#[derive(Debug, PartialEq, Clone)]
pub enum NodeKind {
    Variable(String),
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
    pub value: String,
    pub kind: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Node {
    pub id: usize,
    pub inputs: RefCell<Vec<Rc<Self>>>,
    pub outputs: RefCell<Vec<Rc<Self>>>,
    kind: NodeKind,
}

impl Node {
    pub fn from_variable(node_id: NodeId, variable_name: String) -> Result<Self> {
        Ok(Node {
            id: node_id,
            inputs: RefCell::new(Vec::new()),
            outputs: RefCell::new(Vec::new()),
            kind: NodeKind::Variable(variable_name),
        })
    }

    pub fn from_formula(node_id: NodeId, formula: &str) -> Result<Self> {
        let formula = build_operator_tree(&formula).unwrap();
        Ok(Node {
            id: node_id,
            inputs: RefCell::new(Vec::new()),
            outputs: RefCell::new(Vec::new()),
            kind: NodeKind::Formula(formula),
        })
    }

    pub fn inputs(&self) -> Vec<NodeId> {
        let inputs = self.inputs.borrow();
        if inputs.len() == 0 {
            return vec![self.id];
        }

        let mut ids = Vec::new();
        for input in inputs.iter() {
            let id = &input.inputs();
            ids.extend_from_slice(id);
        }

        return ids;
    }

    pub fn eval(&self, values: &HashMap<NodeId, NodeOutput>) -> Result<NodeOutput> {
        match &self.kind {
            NodeKind::Variable(var_name) => {
                let val = values.get(&self.id).ok_or(anyhow!(
                    "missing variable value for {} (node id = {})",
                    var_name,
                    self.id
                ))?;
                return Ok(val.clone());
            }
            _ => (),
        }
        let mut input_vals = Vec::new();
        let mut node_ids = Vec::new();
        let mut max_len = 0;
        let inputs = self.inputs.borrow_mut();
        for node in inputs.iter() {
            let val = node.eval(values)?;
            let val = match val {
                NodeOutput::Number(v) => vec![v],
                NodeOutput::NumberArray(v) => v,
            };
            max_len = max_len.max(val.len());
            node_ids.push(format!("${}", node.id));
            input_vals.push(val);
        }

        let mut output_vals = Vec::new();
        for idx_arr in 0..max_len {
            match &self.kind {
                NodeKind::Variable(_) => unreachable!(),
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

#[derive(Debug, PartialEq, Clone)]
pub struct Tree {
    nodes: HashMap<usize, Rc<Node>>,
}

impl Tree {
    pub fn new(
        nodes_definitions: Vec<NodeDefinition>,
        edge_definitions: Vec<EdgeDefinition>,
    ) -> Result<Self> {
        let mut nodes = HashMap::new();
        for node_def in &nodes_definitions {
            if let None = nodes.get_mut(&node_def.node_id) {
                let node = match node_def.kind {
                    0 => Rc::new(Node::from_variable(
                        node_def.node_id,
                        node_def.value.clone(),
                    )?),
                    1 => Rc::new(Node::from_formula(node_def.node_id, &node_def.value)?),
                    _ => Err(anyhow!("Invalid node type"))?,
                };

                nodes.insert(node_def.node_id, Rc::clone(&node));
            }
        }

        for edge_def in &edge_definitions {
            let Some(node) = nodes.get(&edge_def.node_id) else {
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

    pub fn node_inputs(&self, node_id: NodeId) -> Result<Vec<String>> {
        let node = self
            .nodes
            .get(&node_id)
            .ok_or(anyhow!("no node with id {}", node_id))?;
        let inputs = node.inputs();
        let res = inputs
            .iter()
            .filter_map(|x| match self.nodes.get(x) {
                Some(node) => match &node.kind {
                    NodeKind::Variable(var_name) => Some(var_name.clone()),
                    _ => None,
                },
                None => None,
            })
            .collect();
        return Ok(res);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree() {
        let edge_defs = vec![
            EdgeDefinition {
                node_id: 2,
                input_id: 0,
            },
            EdgeDefinition {
                node_id: 2,
                input_id: 1,
            },
            EdgeDefinition {
                node_id: 0,
                input_id: 3,
            },
            EdgeDefinition {
                node_id: 1,
                input_id: 4,
            },
        ];
        let node_defs = vec![
            NodeDefinition {
                node_id: 3,
                kind: 0,
                value: "a".into(),
            },
            NodeDefinition {
                node_id: 4,
                kind: 0,
                value: "b".into(),
            },
            NodeDefinition {
                node_id: 0,
                kind: 1,
                value: "a + 1".into(),
            },
            NodeDefinition {
                node_id: 1,
                kind: 1,
                value: "b * 2".into(),
            },
            NodeDefinition {
                node_id: 2,
                kind: 1,
                value: "$0 + $1".into(),
            },
        ];

        let tree = Tree::new(node_defs, edge_defs).unwrap();
        let inputs = tree.node_inputs(2).unwrap();
        assert_eq!(inputs, vec!["a", "b"]);
        //let outputs = tree.node_ouputs(1);
    }

    // #[test]
    // fn test_formula() {
    //     let node1 = Node::from_variable("$1").unwrap();
    //     let node1 = Node::from_variable(1.5).unwrap();
    //     node1.id = 1;
    //     let mut node2 = Node::from_float_vec(&vec![5.5, 9.5]).unwrap();
    //     node2.id = 1;
    //     let mut node3 = Node::from_formula("($1 - $0) / 2").unwrap();
    //     node3.id = 2;
    //     node3.inputs = RefCell::new(vec![Rc::new(node1), Rc::new(node2)]);

    //     let res = node3.eval(HashMap::new()).unwrap();
    //     assert_eq!(res, NodeOutput::NumberArray(vec![2., 4.]));

    //     let mut node4 = Node::from_formula("$2^2").unwrap();
    //     node4.id = 3;
    //     node4.inputs = RefCell::new(vec![Rc::new(node3)]);

    //     let res = node4.eval().unwrap();
    //     assert_eq!(res, NodeOutput::NumberArray(vec![4., 16.]));
    // }
}
