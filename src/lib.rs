use anyhow::{anyhow, Result};
use num::{bigint::Sign, range, BigInt, BigRational};
use savage_core::{
    expression::{Expression, RationalRepresentation},
    helpers::*,
};
use std::collections::HashMap;

#[derive(Debug)]
pub enum NodeKind {
    Number(Expression),
    NumberArray(Vec<Expression>),
    Formula(Expression),
    SqlQuery(String),
}

#[derive(Debug, PartialEq)]
pub enum NodeOutput {
    NumberArray(Vec<Expression>),
    Number(Expression),
}

impl TryFrom<f64> for NodeOutput {
    type Error = anyhow::Error;

    fn try_from(value: f64) -> Result<Self> {
        Ok(NodeOutput::Number(f64_to_expression(value)?))
    }
}

impl TryFrom<Vec<f64>> for NodeOutput {
    type Error = anyhow::Error;

    fn try_from(value: Vec<f64>) -> Result<Self> {
        Ok(NodeOutput::NumberArray(vec_f64_to_expression(&value)?))
    }
}

impl TryInto<f64> for NodeOutput {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<f64> {
        match self {
            Self::Number(expr) => Ok(expression_to_f64(expr)?),
            _ => Err(anyhow!("Unable to convert vector to number")),
        }
    }
}

impl TryInto<Vec<f64>> for NodeOutput {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Vec<f64>> {
        match self {
            Self::NumberArray(vec) => {
                let vec: Result<Vec<f64>> =
                    vec.iter().map(|x| expression_to_f64(x.clone())).collect();
                return vec;
            }
            _ => Err(anyhow!("Unable to convert number to vector")),
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub id: i32,
    pub inputs: Option<Vec<Self>>,
    kind: NodeKind,
}

impl Node {
    pub fn from_float(value: f64) -> Result<Self> {
        Ok(Node {
            id: 0,
            inputs: None,
            kind: NodeKind::Number(f64_to_expression(value)?),
        })
    }

    pub fn from_float_vec(vec: &Vec<f64>) -> Result<Self> {
        Ok(Node {
            id: 0,
            inputs: None,
            kind: NodeKind::NumberArray(vec_f64_to_expression(vec)?),
        })
    }

    pub fn from_formula(formula: &str) -> Result<Self> {
        let Ok(expr) = formula.parse::<Expression>() else {
            return Err(anyhow!("Unable to parse expression"));
        };

        Ok(Node {
            id: 0,
            inputs: None,
            kind: NodeKind::Formula(expr),
        })
    }

    pub fn compute(&self) -> Result<NodeOutput> {
        let mut input_vals = Vec::new();
        let mut node_ids = Vec::new();
        let mut max_len = 0;
        if let Some(nodes) = &self.inputs {
            for node in nodes {
                let val = node.compute()?;
                let val = match val {
                    NodeOutput::Number(v) => vec![v],
                    NodeOutput::NumberArray(v) => v,
                };
                max_len = max_len.max(val.len());
                node_ids.push(format!("id{}", node.id));
                input_vals.push(val);
            }
        }

        match &self.kind {
            NodeKind::Number(_) => max_len = 1,
            NodeKind::NumberArray(_) => max_len = 1,
            _ => (),
        }

        let mut output_vals = Vec::new();
        for idx_arr in range(0, max_len) {
            match &self.kind {
                NodeKind::Number(val) => output_vals.push(val.clone()),
                NodeKind::NumberArray(vec) => output_vals.append(&mut vec.clone()),
                NodeKind::Formula(formula) => {
                    let mut args = HashMap::new();
                    for idx_node in range(0, node_ids.len()) {
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

                        args.insert(id.to_string(), val.clone());
                    }

                    let Ok(res) = formula.evaluate(args) else {
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

fn vec_f64_to_expression(vec: &Vec<f64>) -> Result<Vec<Expression>> {
    vec.iter().map(|x| f64_to_expression(*x)).collect()
}

fn f64_to_expression(number: f64) -> Result<Expression> {
    let Some(expr) = BigRational::from_float(number) else {
        return Err(anyhow!("Unable to convert float to rational"));
    };

    return Ok(rat(expr.numer().clone(), expr.denom().clone()));
}

fn expression_to_f64(expr: Expression) -> Result<f64> {
    match expr {
        Expression::Integer(i) => bigint_to_f64(i),
        Expression::Rational(val, repr) => {
            let numer = bigint_to_f64(val.numer().clone())?;
            let denom = bigint_to_f64(val.denom().clone())?;

            Ok(match repr {
                RationalRepresentation::Decimal => numer / denom,
                RationalRepresentation::Fraction => numer / denom,
            })
        }
        _ => Err(anyhow!("Expression cannot be unpacked")),
    }
}

fn bigint_to_f64(val: BigInt) -> Result<f64> {
    let (sign, nums) = val.to_u32_digits();
    let num = nums.first().ok_or(anyhow!("Expression is empty"))?;
    let num = *num as f64;
    return Ok(match sign {
        Sign::Minus => -num,
        _ => num,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula() {
        let node1 = Node::from_float(1.5).unwrap();
        let mut node2 = Node::from_float_vec(&vec![5.5, 9.5]).unwrap();
        node2.id = 1;
        let mut node3 = Node::from_formula("(id1 - id0) / 2").unwrap();
        node3.id = 2;
        node3.inputs = Some(vec![node1, node2]);

        let res = TryInto::<Vec<f64>>::try_into(node3.compute().unwrap())
            .unwrap();
        assert_eq!(res, vec![2., 4.]);

        let mut node4 = Node::from_formula("id2^2").unwrap();
        node4.id = 3;
        node4.inputs = Some(vec![node3]);

        let res = TryInto::<Vec<f64>>::try_into(node4.compute().unwrap())
            .unwrap();
        assert_eq!(res, vec![4., 16.]);
    }
}
