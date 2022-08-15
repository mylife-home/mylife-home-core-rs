pub mod metadata;

use std::ops::Deref;

pub trait Plugin {
    fn runtime_data(&self) -> (&[&dyn StateDef], &[&dyn ActionDef], &[&dyn ConfigDef]);
    fn init(&mut self) {}
    fn terminate(&mut self) {}
}

pub struct PluginData {
    metadata: metadata::PluginMetadata,
    factory: fn() -> Box<dyn Plugin>,
}

impl PluginData {
    pub fn new(metadata: metadata::PluginMetadata, factory: fn() -> Box<dyn Plugin>) -> PluginData {
        PluginData { metadata, factory }
    }

    pub fn metadata(&self) -> &metadata::PluginMetadata {
        &self.metadata
    }

    pub fn create(&self) -> Box<dyn Plugin> {
        (self.factory)()
    }
}

pub trait StateDef {}

pub struct State<T> {
    value: T,
    // callbacks
}

impl<T> State<T> {
    pub fn change(&mut self, value: T) {
        self.value = value;
        // callbacks
    }
}

impl<T> StateDef for State<T> {}

impl<T> Deref for State<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub trait ConfigDef {}

pub struct Config<T> {
    value: T,
    // init
}

impl<T> ConfigDef for Config<T> {}

impl<T> Deref for Config<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub trait ActionDef {}

pub struct Action<T> {
    value: T,
    // handler
}

impl<T> ActionDef for Action<T> {}
