use std::collections::{
    hash_map::{Entry, Keys},
    HashMap, HashSet,
};
use std::iter::Copied;
use std::slice::Iter;

use ola::DmxBuffer;
use petgraph::{
    visit::{
        Data, GraphBase, IntoNeighbors, IntoNeighborsDirected, IntoNodeIdentifiers, NodeCount,
        Visitable,
    },
    Direction,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::trace;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Node is not an input")]
    Input,
    #[error("No output with id {0}")]
    Subscribe(Uuid),
    #[error("Node already present with id {0}")]
    Insert(Uuid),
}

#[derive(Clone, Debug, Default)]
pub struct SceneGraph(HashMap<Uuid, Node>);

impl SceneGraph {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn insert(&mut self, id: Uuid, node: Node) -> Result<(), Error> {
        if let Entry::Vacant(n) = self.0.entry(id) {
            let _ = n.insert(node);

            Ok(())
        } else {
            Err(Error::Insert(id))
        }
    }

    pub fn subscribe(&mut self, output: &Uuid, tx: mpsc::Sender<DmxBuffer>) -> Result<(), Error> {
        if let Some(Node::Output { subscribers, .. }) = self.0.get_mut(output) {
            trace!("registering new subscriber with output {}", output);
            subscribers.push(tx);

            Ok(())
        } else {
            Err(Error::Subscribe(*output))
        }
    }
}

impl GraphBase for SceneGraph {
    type NodeId = Uuid;
    type EdgeId = (Uuid, Uuid);
}

impl Data for SceneGraph {
    type NodeWeight = Node;
    type EdgeWeight = ();
}

impl NodeCount for SceneGraph {
    fn node_count(&self) -> usize {
        self.0.len()
    }
}

impl Visitable for SceneGraph {
    type Map = HashSet<Uuid>;

    fn visit_map(&self) -> Self::Map {
        HashSet::with_capacity(self.node_count())
    }

    fn reset_map(&self, map: &mut Self::Map) {
        map.clear();
    }
}

impl<'a> IntoNodeIdentifiers for &'a SceneGraph {
    type NodeIdentifiers = Copied<Keys<'a, Uuid, Node>>;

    fn node_identifiers(self) -> Self::NodeIdentifiers {
        self.0.keys().copied()
    }
}

impl<'a> IntoNeighbors for &'a SceneGraph {
    type Neighbors = Neighbors<'a>;

    fn neighbors(self, a: Self::NodeId) -> Self::Neighbors {
        // petgraph panics if a is out of bounds
        let node = self.0.get(&a).unwrap();

        node.neighbors()
    }
}

impl<'a> IntoNeighborsDirected for &'a SceneGraph {
    type NeighborsDirected = NeighborsDirected<'a>;

    fn neighbors_directed(self, a: Self::NodeId, dir: Direction) -> Self::NeighborsDirected {
        // petgraph panics if a is out of bounds
        let node = self.0.get(&a).unwrap();

        match dir {
            Direction::Outgoing => node.outgoing(),
            Direction::Incoming => node.incoming(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Node {
    Input {
        outputs: Vec<Uuid>,
        channels: DmxBuffer,
    },
    Output {
        input: Uuid,
        subscribers: Vec<mpsc::Sender<DmxBuffer>>,
    },
}

impl<'a> Node {
    fn get(&'a self, i: usize) -> Option<Uuid> {
        match self {
            Node::Input { .. } => None,
            Node::Output { input, .. } => match i {
                0 => Some(*input),
                _ => None,
            },
        }
    }

    fn incoming_size_hint(&'a self) -> (usize, Option<usize>) {
        match self {
            Node::Input { .. } => (0, Some(0)),
            Node::Output { .. } => (1, Some(1)),
        }
    }
}

impl<'a> Node {
    fn outgoing(&'a self) -> NeighborsDirected<'a> {
        match self {
            Node::Input { outputs, .. } => {
                NeighborsDirected::Outgoing(Some(outputs.iter().copied()))
            }
            Node::Output { .. } => NeighborsDirected::Outgoing(None),
        }
    }

    fn incoming(&'a self) -> NeighborsDirected<'a> {
        NeighborsDirected::Incoming {
            index: match self {
                Node::Input { .. } => 0,
                Node::Output { .. } => 1,
            },
            node: self,
        }
    }

    fn neighbors(&'a self) -> Neighbors<'a> {
        Neighbors {
            inputs: Some(self.incoming()),
            outputs: self.outgoing(),
        }
    }
}

pub struct Neighbors<'a> {
    inputs: Option<NeighborsDirected<'a>>,
    outputs: NeighborsDirected<'a>,
}

impl<'a> Iterator for Neighbors<'a> {
    type Item = Uuid;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inputs) = &mut self.inputs {
            if let Some(next) = inputs.next() {
                return Some(next);
            }

            self.inputs = None;
        }

        self.outputs.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if let Some(inputs) = &self.inputs {
            match (self.outputs.size_hint(), inputs.size_hint()) {
                ((l_out, None), (l_in, None)) => (l_out + l_in, None),
                ((l_out, Some(u_out)), (l_in, None)) => (l_out + l_in, Some(u_out)),
                ((l_out, None), (l_in, Some(u_in))) => (l_out + l_in, Some(u_in)),
                ((l_out, Some(u_out)), (l_in, Some(u_in))) => (l_out + l_in, Some(u_out + u_in)),
            }
        } else {
            self.outputs.size_hint()
        }
    }
}

pub enum NeighborsDirected<'a> {
    Outgoing(Option<Copied<Iter<'a, Uuid>>>),
    Incoming { index: usize, node: &'a Node },
}

impl<'a> Iterator for NeighborsDirected<'a> {
    type Item = Uuid;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            NeighborsDirected::Outgoing(iter) => iter.as_mut().and_then(|i| i.next()),
            NeighborsDirected::Incoming { index, node } => match index {
                0 => None,
                _ => {
                    *index -= 1;
                    node.get(*index)
                }
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            NeighborsDirected::Outgoing(iter) => {
                iter.as_ref().map(|i| i.size_hint()).unwrap_or((0, Some(0)))
            }
            NeighborsDirected::Incoming { node, .. } => node.incoming_size_hint(),
        }
    }
}
