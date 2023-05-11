use std::collections::{
    hash_map::{self, Entry},
    HashMap, VecDeque,
};

use crate::node::{self, Node};
use crate::subscription::{self, GraphUpdate, Subscription};
use crate::transaction::{MapLike, Transaction};

use generational_arena::{Arena, Index};
use ola::DmxBuffer;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Input {0} does not exist")]
    MissingInput(u32),
    #[error("A node does not exist with the given id")]
    UnknownNode,
    #[error("A subscription does not exist with the given id")]
    UnknownSubscription,
    #[error("{0} while setting up subscription")]
    Subscribe(#[from] subscription::Error),
    #[error("Operation would create a dependency cycle")]
    Cycle,
}

#[derive(Clone, Debug, Default)]
pub struct SceneGraph {
    subscriptions: HashMap<Uuid, Subscription>,
    nodes: HashMap<Uuid, Node>,
    node_states: HashMap<Uuid, DmxBuffer>,
    dependencies: HashMap<Uuid, Dependencies>,
}

impl SceneGraph {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn insert(&mut self, id: Uuid, node: Node) -> Result<(), Error> {
        let mut dependencies = Transaction::new(&mut self.dependencies);
        let reverse = match dependencies.get_mut(&id) {
            Some(node) => {
                let node = node.clone();
                Self::disconnect_forward(&mut dependencies, &id, &node.forward);
                node.reverse
            }
            None => Arena::new(),
        };

        let mut forward = Vec::new();
        for (index, dependency_id) in node.dependencies().iter().enumerate() {
            if let Some(dependency_id) = dependency_id {
                if let Some(dependencies) = dependencies.get_mut(dependency_id) {
                    let forward_index = dependencies.reverse.insert(Dependent::Node {
                        id,
                        index: index as u32,
                    });
                    forward.push(Some((*dependency_id, forward_index)));
                } else {
                    error!(
                        "missing input {} of {} found while inserting {}",
                        index, dependency_id, id
                    );
                    return Err(Error::MissingInput(index as u32));
                }
            } else {
                forward.push(None);
            }
        }

        let mut nodes = Transaction::new(&mut self.nodes);
        nodes.insert(id, node);
        dependencies.insert(id, Dependencies { forward, reverse });

        Self::update(
            &id,
            nodes,
            Transaction::new(&mut self.node_states),
            dependencies,
            &mut self.subscriptions,
        )
        .await
    }

    pub async fn remove(&mut self, id: Uuid) -> Result<(), Error> {
        if let Entry::Occupied(occupied) = self.nodes.entry(id) {
            occupied.remove();

            if let Some(dependencies) = self.dependencies.get(&id) {
                let Dependencies { forward, reverse } = dependencies.clone();

                Self::disconnect_forward(&mut self.dependencies, &id, &forward);
                self.disconnect_reverse(&id, &reverse).await;
            } else {
                warn!("dependency not found for subscription {}", id);
            };

            Ok(())
        } else {
            Err(Error::UnknownNode)
        }
    }

    pub fn get(&self, id: &Uuid) -> Result<&Node, Error> {
        if let Some(node) = self.nodes.get(id) {
            Ok(node)
        } else {
            Err(Error::UnknownNode)
        }
    }

    pub fn iter(&self) -> hash_map::Iter<Uuid, Node> {
        self.nodes.iter()
    }

    fn disconnect_forward<D>(dependencies: &mut D, id: &Uuid, forward: &[Option<(Uuid, Index)>])
    where
        D: MapLike<Uuid, Dependencies>,
    {
        for (dependency_id, index) in forward.iter().flatten() {
            if let Some(dependency) = dependencies.get_mut(dependency_id) {
                dependency.reverse.remove(*index);
            } else {
                warn!(
                    "missing dependency {} encountered while removing {}",
                    dependency_id, id
                );
            }
        }
    }

    async fn disconnect_reverse(&mut self, id: &Uuid, reverse: &Arena<Dependent>) {
        for (_, dependent) in reverse {
            match dependent {
                Dependent::Node { id: node_id, index } => match self.nodes.get_mut(node_id) {
                    Some(node) => {
                        node.unlink(*index);
                        self.dependencies
                            .get_mut(node_id)
                            .expect("get dependencies of updated node")
                            .forward[*index as usize] = None;
                        if let Err(e) = Self::update(
                            node_id,
                            Transaction::new(&mut self.nodes),
                            Transaction::new(&mut self.node_states),
                            Transaction::new(&mut self.dependencies),
                            &mut self.subscriptions,
                        )
                        .await
                        {
                            warn!("while disconnecting dependent nodes: {}", e);
                        };
                    }
                    None => warn!(
                        "missing dependent {} encountered while removing {}",
                        node_id, id
                    ),
                },
                Dependent::Subscription {
                    id: subscription_id,
                } => match self.subscriptions.remove(subscription_id) {
                    Some(subscription) => subscription.close().await,
                    None => warn!(
                        "missing subscription {} encountered while removing {}",
                        subscription_id, id
                    ),
                },
            }
        }
    }

    async fn update<'a>(
        id: &Uuid,
        mut nodes: Transaction<'a, Uuid, Node>,
        mut node_states: Transaction<'a, Uuid, DmxBuffer>,
        mut dependencies: Transaction<'a, Uuid, Dependencies>,
        subscriptions: &mut HashMap<Uuid, Subscription>,
    ) -> Result<(), Error> {
        let mut updates = Vec::new();

        let mut targets = VecDeque::new();
        targets.push_back(Dependent::Node {
            id: *id,
            index: Default::default(),
        });

        while let Some(target) = targets.pop_front() {
            match target {
                Dependent::Node { id: node_id, .. } => {
                    if let Some(node) = nodes.get(&node_id) {
                        match node.update(&node_states) {
                            Ok(state) => {
                                node_states.insert(node_id, state);
                                let dependents = &dependencies
                                    .get(&node_id)
                                    .expect("get dependents of updated node")
                                    .reverse;
                                targets.reserve(dependents.len());
                                for (_, dependent) in dependents.iter() {
                                    match dependent {
                                        Dependent::Node { id: dep_id, .. } if dep_id == id => {
                                            return Err(Error::Cycle)
                                        }
                                        _ => targets.push_back(dependent.clone()),
                                    }
                                }
                            }
                            Err(node::Error::NoInput(index)) => {
                                warn!("node {} had an invalid input {}, unlinking", node_id, index);
                                nodes
                                    .get_mut(&node_id)
                                    .expect("get contents of updated node")
                                    .unlink(index);
                                dependencies
                                    .get_mut(&node_id)
                                    .expect("get dependencies of updated node")
                                    .forward[index as usize] = None;
                                targets.push_front(target);
                            }
                        }
                    } else {
                        return Err(Error::UnknownNode);
                    }
                }
                Dependent::Subscription {
                    id: subscription_id,
                } => updates.push(subscription_id),
            }
        }

        for update in updates {
            if let Some(subscription) = subscriptions.get_mut(&update) {
                subscription.update(&node_states).await?;
            } else {
                warn!("failed to update missing subscription {}", update);
            }
        }

        nodes.commit();
        node_states.commit();
        dependencies.commit();

        Ok(())
    }

    pub async fn subscribe(
        &mut self,
        input: Uuid,
        channel: mpsc::Sender<GraphUpdate>,
    ) -> Result<Uuid, Error> {
        if let Some(input_dependencies) = self.dependencies.get_mut(&input) {
            let id = Uuid::new_v4(); // slow...

            let index = input_dependencies
                .reverse
                .insert(Dependent::Subscription { id });
            self.subscriptions.insert(
                id,
                Subscription::new(id, input, index, &self.node_states, channel).await?,
            );

            Ok(id)
        } else {
            Err(Error::UnknownNode)
        }
    }

    pub fn unsubscribe(&mut self, id: Uuid) -> Result<(), Error> {
        if let Entry::Occupied(occupied) = self.subscriptions.entry(id) {
            let subscription = occupied.get();
            if let Some(input_dependencies) = self.dependencies.get_mut(&subscription.input) {
                input_dependencies.reverse.remove(subscription.index);
            } else {
                warn!("dependency not found for subscription {}", id);
            }

            occupied.remove();

            Ok(())
        } else {
            Err(Error::UnknownSubscription)
        }
    }
}

#[derive(Clone, Debug)]
struct Dependencies {
    forward: Vec<Option<(Uuid, Index)>>,
    reverse: Arena<Dependent>,
}

#[derive(Clone, Debug)]
enum Dependent {
    Node { id: Uuid, index: u32 },
    Subscription { id: Uuid },
}
