use std::collections::{BTreeSet, HashMap};

/// Strongly connected components with more than one node (import cycles), via an
/// iterative Tarjan so deep import chains cannot overflow the stack.
/// Members are sorted and the list is sorted, so output is deterministic.
pub fn sccs(nodes: &[String], edges: &BTreeSet<(String, String)>) -> Vec<Vec<String>> {
    let ix: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut adj = vec![Vec::new(); nodes.len()];
    for (a, b) in edges {
        if let (Some(&x), Some(&y)) = (ix.get(a.as_str()), ix.get(b.as_str())) {
            adj[x].push(y);
        }
    }
    const UNSEEN: usize = usize::MAX;
    let (mut index, mut low) = (vec![UNSEEN; nodes.len()], vec![0; nodes.len()]);
    let mut on_stack = vec![false; nodes.len()];
    let (mut stack, mut next, mut out) = (Vec::new(), 0, Vec::new());
    for root in 0..nodes.len() {
        if index[root] != UNSEEN {
            continue;
        }
        let mut call = vec![(root, 0usize)];
        while let Some(&mut (v, ref mut ei)) = call.last_mut() {
            if *ei == 0 && index[v] == UNSEEN {
                (index[v], low[v]) = (next, next);
                next += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if let Some(&w) = adj[v].get(*ei) {
                *ei += 1;
                if index[w] == UNSEEN {
                    call.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                let mut comp = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp.push(nodes[w].clone());
                    if w == v {
                        break;
                    }
                }
                if comp.len() > 1 {
                    comp.sort();
                    out.push(comp);
                }
            }
        }
    }
    out.sort();
    out
}
