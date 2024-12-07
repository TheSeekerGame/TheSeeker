use crate::prelude::*;

pub struct ScriptLabelPlugin;

impl Plugin for ScriptLabelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EntityLabels>();
    }
}

#[derive(Resource, Default)]
pub struct EntityLabels {
    e2l: HashMap<Entity, HashSet<String>>,
    l2e: HashMap<String, HashSet<Entity>>,
}

impl EntityLabels {
    pub fn insert(&mut self, entity: Entity, label: &str) {
        if let Some(entities) = self.l2e.get_mut(label) {
            entities.insert(entity);
        } else {
            let mut new = HashSet::default();
            new.insert(entity);
            self.l2e.insert(label.to_owned(), new);
        }
        if let Some(labels) = self.e2l.get_mut(&entity) {
            labels.insert(label.to_owned());
        } else {
            let mut new = HashSet::default();
            new.insert(label.to_owned());
            self.e2l.insert(entity, new);
        }
    }

    pub fn remove_label(&mut self, label: &str) {
        if let Some(entities) = self.l2e.remove(label) {
            for entity in entities {
                if let Some(labels) = self.e2l.get_mut(&entity) {
                    labels.remove(label);
                }
            }
        }
    }

    pub fn remove_entity(&mut self, entity: Entity) {
        if let Some(labels) = self.e2l.remove(&entity) {
            for label in labels {
                if let Some(entities) = self.l2e.get_mut(&label) {
                    entities.remove(&entity);
                }
            }
        }
    }

    pub fn remove_entity_label(&mut self, entity: Entity, label: &str) {
        if let Some(labels) = self.e2l.get_mut(&entity) {
            labels.remove(label);
        }
        if let Some(entities) = self.l2e.get_mut(label) {
            entities.remove(&entity);
        }
    }

    pub fn iter_entity_labels(
        &self,
        entity: Entity,
    ) -> impl Iterator<Item = &str> {
        self.e2l
            .get(&entity)
            .into_iter()
            .flat_map(|labels| labels.iter().map(|s| s.as_str()))
    }

    pub fn iter_label_entities(
        &self,
        label: &str,
    ) -> impl Iterator<Item = &Entity> {
        self.l2e
            .get(label)
            .into_iter()
            .flat_map(|entities| entities.iter())
    }
}
