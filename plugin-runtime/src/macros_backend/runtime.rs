use std::{collections::HashMap, fmt, sync::Arc};

use crate::{
    metadata::PluginMetadata,
    runtime::{Config, ConfigValue, MylifeComponent, MylifePluginRuntime, Value},
    MylifePlugin,
};

pub struct PluginRuntimeImpl<PluginType: MylifePlugin + 'static> {
    metadata: PluginMetadata,
    access: Arc<PluginRuntimeAccess<PluginType>>,
}

impl<PluginType: MylifePlugin + 'static> PluginRuntimeImpl<PluginType> {
    pub fn new(
        metadata: PluginMetadata,
        access: Arc<PluginRuntimeAccess<PluginType>>,
    ) -> Box<Self> {
        Box::new(PluginRuntimeImpl { metadata, access })
    }
}

impl<PluginType: MylifePlugin> MylifePluginRuntime for PluginRuntimeImpl<PluginType> {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn create(&self) -> Box<dyn MylifeComponent> {
        ComponentImpl::<PluginType>::new(&self.access)
    }
}

pub type ConfigRuntimeSetter<PluginType> =
    fn(target: &mut PluginType, config: ConfigValue) -> Result<(), Box<dyn std::error::Error>>;
pub type StateRuntimeRegister<PluginType> =
    fn(target: &mut PluginType, listener: Box<dyn Fn(/*state:*/ Value)>) -> ();
pub type ActionRuntimeExecutor<PluginType> =
    fn(target: &mut PluginType, action: Value) -> Result<(), Box<dyn std::error::Error>>;

pub struct PluginRuntimeAccess<PluginType: MylifePlugin> {
    configs: HashMap<String, ConfigRuntimeSetter<PluginType>>,
    states: HashMap<String, StateRuntimeRegister<PluginType>>,
    actions: HashMap<String, ActionRuntimeExecutor<PluginType>>,
}

impl<PluginType: MylifePlugin> PluginRuntimeAccess<PluginType> {
    pub fn new(
        configs: HashMap<String, ConfigRuntimeSetter<PluginType>>,
        states: HashMap<String, StateRuntimeRegister<PluginType>>,
        actions: HashMap<String, ActionRuntimeExecutor<PluginType>>,
    ) -> Arc<Self> {
        Arc::new(PluginRuntimeAccess {
            configs,
            states,
            actions,
        })
    }
}

struct ComponentImpl<PluginType: MylifePlugin> {
    access: Arc<PluginRuntimeAccess<PluginType>>,
    component: PluginType,
    fail_handler: Option<Box<dyn Fn(/*error:*/ Box<dyn std::error::Error>)>>,
    state_handler: Option<Box<dyn Fn(/*name:*/ &str, /*state:*/ Value)>>,
}

impl<PluginType: MylifePlugin> ComponentImpl<PluginType> {
    pub fn new(access: &Arc<PluginRuntimeAccess<PluginType>>) -> Box<Self> {
        let mut component = Box::new(ComponentImpl {
            access: access.clone(),
            component: PluginType::default(),
            fail_handler: None,
            state_handler: None,
        });

        for (name, register) in access.states.iter() {
            register(
                &mut component.component,
                Box::new(|value: Value| {
                    if let Some(handler) = &component.state_handler {
                        handler(name, value);
                    }
                }),
            );
        }

        component
    }

    fn configure_with_res(&mut self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        for (name, setter) in self.access.configs.iter() {
            let value = config
                .get(name)
                .ok_or_else(|| {
                    Box::new(ConfigNotSetError {
                        name: String::from(name),
                    })
                })?
                .clone();

            setter(&mut self.component, value)?;
        }

        Ok(())
    }

    fn execute_action_with_res(
        &mut self,
        name: &str,
        action: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let handler = self.access.actions.get(name).ok_or_else(|| {
            Box::new(NoSuchActionError {
                name: String::from(name),
            })
        })?;

        handler(&mut self.component, action)
    }

    fn res_to_fail<T>(&self, result: Result<T, Box<dyn std::error::Error>>) -> Option<T> {
        let fail_handler = self
            .fail_handler
            .as_ref()
            .expect("Cannot report error without registered fail handler");

        match result {
            Ok(value) => Some(value),
            Err(error) => {
                fail_handler(error);
                None
            }
        }
    }
}

impl<PluginType: MylifePlugin> MylifeComponent for ComponentImpl<PluginType> {
    fn set_on_fail(&mut self, handler: Box<dyn Fn(/*error:*/ Box<dyn std::error::Error>)>) {
        self.fail_handler = Some(handler);
    }

    fn set_on_state(&mut self, handler: Box<dyn Fn(/*name:*/ &str, /*state:*/ Value)>) {
        self.state_handler = Some(handler);
    }

    fn configure(&mut self, config: &Config) {
        let result = self.configure_with_res(config);
        self.res_to_fail(result);
    }

    fn execute_action(&mut self, name: &str, action: Value) {
        let result = self.execute_action_with_res(name, action);
        self.res_to_fail(result);
    }

    fn init(&mut self) {
        let result = self.component.init();
        self.res_to_fail(result);
    }
}

#[derive(Debug, Clone)]
pub struct ConfigNotSetError {
    name: String,
}

impl std::error::Error for ConfigNotSetError {}

impl fmt::Display for ConfigNotSetError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Config key not set: '{}'", self.name)
    }
}

#[derive(Debug, Clone)]
pub struct NoSuchActionError {
    name: String,
}

impl std::error::Error for NoSuchActionError {}

impl fmt::Display for NoSuchActionError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "No such action: '{}'", self.name)
    }
}
